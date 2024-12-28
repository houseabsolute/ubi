use crate::{
    assets::{Asset, Assets},
    checksums,
    forge::Forge,
    installer::Installer,
    picker::AssetPicker,
};
use anyhow::{anyhow, Result};
use log::debug;
use reqwest::{
    header::{HeaderValue, ACCEPT},
    Client, StatusCode,
};
use std::{fs::File, io::Write, path::PathBuf};
use tempfile::{tempdir, TempDir};
use url::Url;

/// `Ubi` is the core of this library, and is used to download and install a binary. Use the
/// [`UbiBuilder`](crate::UbiBuilder) struct to create a new `Ubi` instance.
#[derive(Debug)]
pub struct Ubi<'a> {
    forge: Box<dyn Forge + Send + Sync>,
    asset_url: Option<Url>,
    asset_picker: AssetPicker<'a>,
    installer: Installer,
    reqwest_client: Client,
}

#[derive(Debug)]
pub(crate) struct Download {
    // We need to keep the temp dir around so that it's not deleted before
    // we're done with it.
    pub(crate) _temp_dir: TempDir,
    pub(crate) path: PathBuf,
}

impl<'a> Ubi<'a> {
    /// Create a new Ubi instance.
    pub(crate) fn new(
        forge: Box<dyn Forge + Send + Sync>,
        asset_url: Option<Url>,
        asset_picker: AssetPicker<'a>,
        installer: Installer,
        reqwest_client: Client,
    ) -> Ubi<'a> {
        Ubi {
            forge,
            asset_url,
            asset_picker,
            installer,
            reqwest_client,
        }
    }

    /// Install the binary. This will download the appropriate release asset from GitHub and unpack
    /// it. It will look for an executable (based on the name of the project or the explicitly set
    /// executable name) in the unpacked archive and write it to the install directory. It will also
    /// set the executable bit on the installed binary on platforms where this is necessary.
    ///
    /// # Errors
    ///
    /// There are a number of cases where an error can be returned:
    ///
    /// * Network errors on requests to the forge site (GitHub, GitLab, etc.).
    /// * You've reached the API limits for the forge site (try setting the appropriate token env var
    ///   to increase these).
    /// * Unable to find the requested project.
    /// * Unable to find a match for the platform on which the code is running.
    /// * Unable to unpack/uncompress the downloaded release file.
    /// * Unable to find an executable with the right name in a downloaded archive.
    /// * Unable to write the executable to the specified directory.
    /// * Unable to set executable permissions on the installed binary.
    pub async fn install_binary(&mut self) -> Result<()> {
        let (asset, checksum_asset) = self.asset().await?;
        let download = self.download_asset(&self.reqwest_client, asset).await?;
        if let Some(checksum_asset) = checksum_asset {
            let checksum_download = self
                .download_asset(&self.reqwest_client, checksum_asset)
                .await?;
            checksums::verify(&download, &checksum_download)?;
        } else {
            debug!("did not find a checksum asset to download");
        }
        self.installer.install(&download)
    }

    pub(crate) async fn asset(&mut self) -> Result<(Asset, Option<Asset>)> {
        if let Some(url) = &self.asset_url {
            return Ok((
                Asset {
                    name: url.path().split('/').last().unwrap().to_string(),
                    url: url.clone(),
                },
                None,
            ));
        }

        let mut assets = self.forge.fetch_assets(&self.reqwest_client).await?;
        let name = self.asset_picker.pick_asset(assets.keys())?.to_owned();
        debug!("picked asset named {name}");
        let (name, url) = assets.remove_entry(&name).unwrap();
        let checksum_asset = Self::maybe_find_checksum_asset(&name, assets);
        Ok((Asset { name, url }, checksum_asset))
    }

    fn maybe_find_checksum_asset(name: &str, mut assets: Assets) -> Option<Asset> {
        let checksum_name = checksums::find_checksum_asset_for(name, assets.keys());
        match checksum_name {
            Some(checksum_name) => {
                let (name, url) = assets.remove_entry(&checksum_name).unwrap();
                Some(Asset { name, url })
            }
            None => None,
        }
    }

    async fn download_asset(&self, client: &Client, asset: Asset) -> Result<Download> {
        debug!("downloading asset from {}", asset.url);

        let mut req_builder = client
            .get(asset.url.clone())
            .header(ACCEPT, HeaderValue::from_str("application/octet-stream")?);
        req_builder = self.forge.maybe_add_token_header(req_builder)?;
        let req = req_builder.build()?;

        let mut resp = self.reqwest_client.execute(req).await?;
        if resp.status() != StatusCode::OK {
            let mut msg = format!("error requesting {}: {}", asset.url, resp.status());
            if let Ok(t) = resp.text().await {
                msg.push('\n');
                msg.push_str(&t);
            }
            return Err(anyhow!(msg));
        }

        let td = tempdir()?;
        let mut download_path = td.path().to_path_buf();
        download_path.push(&asset.name);
        debug!("archive path is {}", download_path.to_string_lossy());

        {
            let mut downloaded_file = File::create(&download_path)?;
            while let Some(c) = resp.chunk().await? {
                downloaded_file.write_all(c.as_ref())?;
            }
        }

        Ok(Download {
            _temp_dir: td,
            path: download_path,
        })
    }
}
