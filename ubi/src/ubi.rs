use crate::{forge::Forge, installer::Installer, picker::AssetPicker};
use anyhow::{anyhow, Context, Result};
use log::debug;
use reqwest::{
    header::{HeaderValue, ACCEPT},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Write, path::PathBuf};
use tempfile::{tempdir, TempDir};
use url::Url;

/// `Ubi` is the core of this library, and is used to download and install a binary. Use the
/// [`UbiBuilder`](crate::UbiBuilder) struct to create a new `Ubi` instance.
#[derive(Debug)]
pub struct Ubi<'a> {
    forge: Forge,
    asset_url: Option<Url>,
    asset_picker: AssetPicker<'a>,
    installer: Box<dyn Installer>,
    reqwest_client: Client,
    min_age_days: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(try_from = "AssetHelper")]
pub(crate) struct Asset {
    pub(crate) name: String,
    pub(crate) url: Url,
}

#[derive(Debug, Deserialize)]
struct AssetHelper {
    name: String,
    url: Option<Url>,
    browser_download_url: Option<Url>,
}

impl TryFrom<AssetHelper> for Asset {
    type Error = anyhow::Error;

    fn try_from(helper: AssetHelper) -> Result<Self, Self::Error> {
        // prefer `url` (API endpoint) over `browser_download_url` because the API endpoint
        // works for both public and private repos with proper authentication headers, while
        // the browser download URL only works for public repos.
        let url = helper.url.or(helper.browser_download_url).ok_or(anyhow!(
            "an asset in the response did not have a `url` or `browser_download_url` field"
        ))?;

        Ok(Asset {
            name: helper.name,
            url,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Download {
    // We need to keep the temp dir around so that it's not deleted before
    // we're done with it.
    pub(crate) _temp_dir: TempDir,
    pub(crate) archive_path: PathBuf,
}

impl<'a> Ubi<'a> {
    /// Create a new Ubi instance.
    pub(crate) fn new(
        forge: Forge,
        asset_url: Option<Url>,
        asset_picker: AssetPicker<'a>,
        installer: Box<dyn Installer>,
        reqwest_client: Client,
        min_age_days: Option<u32>,
    ) -> Ubi<'a> {
        Ubi {
            forge,
            asset_url,
            asset_picker,
            installer,
            reqwest_client,
            min_age_days,
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
        let asset = self.asset().await?;
        let download = self.download_asset(&self.reqwest_client, asset).await?;
        self.installer.install(&download)
    }

    pub(crate) async fn asset(&mut self) -> Result<Asset> {
        if let Some(url) = &self.asset_url {
            // URL mode: skip age check
            return Ok(Asset {
                name: url.path().split('/').next_back().unwrap().to_string(),
                url: url.clone(),
            });
        }

        let assets = if let Some(min_age) = self.min_age_days {
            // Minimum age mode: fetch with age filtering
            self.forge
                .fetch_assets_with_min_age(&self.reqwest_client, min_age)
                .await?
        } else {
            // Normal mode: fetch latest
            self.forge.fetch_assets(&self.reqwest_client).await?
        };

        let asset = self.asset_picker.pick_asset(assets)?;
        debug!("picked asset named {}", asset.name);
        Ok(asset)
    }

    async fn download_asset(&self, client: &Client, asset: Asset) -> Result<Download> {
        debug!("downloading asset from {}", asset.url);

        let mut req_builder = client.get(asset.url.clone()).header(
            ACCEPT,
            HeaderValue::from_str("application/octet-stream")
                .context("failed to create header value for Accept header")?,
        );
        req_builder = self.forge.maybe_add_token_header(req_builder)?;
        let req = req_builder
            .build()
            .with_context(|| format!("failed to build HTTP request for {}", asset.url))?;

        let mut resp = self.reqwest_client.execute(req).await.with_context(|| {
            format!(
                "failed to execute HTTP request to download asset from {}",
                asset.url
            )
        })?;
        if resp.status() != StatusCode::OK {
            let mut msg = format!("error requesting {}: {}", asset.url, resp.status());
            if let Ok(t) = resp.text().await {
                msg.push('\n');
                msg.push_str(&t);
            }
            return Err(anyhow!(msg));
        }

        let td = tempdir().context("failed to create temporary directory for download")?;
        let mut archive_path = td.path().to_path_buf();
        archive_path.push(&asset.name);
        debug!("archive path is {}", archive_path.to_string_lossy());

        {
            let mut downloaded_file = File::create(&archive_path).with_context(|| {
                format!(
                    "failed to create file at {} for downloaded asset",
                    archive_path.display()
                )
            })?;
            while let Some(c) = resp.chunk().await.with_context(|| {
                format!(
                    "failed to read chunk while downloading asset from {}",
                    asset.url
                )
            })? {
                downloaded_file.write_all(c.as_ref()).with_context(|| {
                    format!("failed to write chunk to {}", archive_path.display())
                })?;
            }
        }

        Ok(Download {
            _temp_dir: td,
            archive_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    struct AssetTextInput {
        url: Option<&'static str>,
        browser_download_url: Option<&'static str>,
    }

    enum AssetTextExpect {
        Success(&'static str),
        Fail,
    }

    #[rstest]
    #[case::prefers_url_when_both_are_present(
        AssetTextInput{
            url: Some("https://api.github.com/repos/owner/repo/releases/assets/123"),
            browser_download_url: Some("https://github.com/owner/repo/releases/download/v1.0.0/asset.tar.gz"),
        },
        AssetTextExpect::Success("https://api.github.com/repos/owner/repo/releases/assets/123"),
    )]
    #[case::usess_browser_download_url_when_url_is_absent(
        AssetTextInput{
            url: None,
            browser_download_url: Some("https://github.com/owner/repo/releases/download/v1.0.0/asset.tar.gz"),
        },
        AssetTextExpect::Success("https://github.com/owner/repo/releases/download/v1.0.0/asset.tar.gz"),
    )]
    #[case::uses_url_when_browser_download_url_is_absent(
        AssetTextInput{
            url: Some("https://api.github.com/repos/owner/repo/releases/assets/123"),
            browser_download_url: None,
        },
        AssetTextExpect::Success("https://api.github.com/repos/owner/repo/releases/assets/123"),
    )]
    #[case::returns_error_when_both_urls_are_absent(
        AssetTextInput{
            url: None,
            browser_download_url: None,
        },
        AssetTextExpect::Fail,
    )]
    fn asset_prefers_api_url_over_browser_download_url(
        #[case] input: AssetTextInput,
        #[case] expect: AssetTextExpect,
    ) -> Result<()> {
        // This test ensures we prefer the API endpoint URL (`url`) over `browser_download_url`
        // because the API endpoint works for both public AND private repos with authentication,
        // while browser_download_url only works for public repos.

        // When both URLs are present, prefer the API endpoint URL
        let helper = AssetHelper {
            name: "asset.tar.gz".to_string(),
            url: input.url.map(Url::parse).transpose()?,
            browser_download_url: input.browser_download_url.map(Url::parse).transpose()?,
        };
        let asset = Asset::try_from(helper);

        match expect {
            AssetTextExpect::Success(url) => {
                let asset = asset?;
                assert_eq!(asset.url.as_str(), url);
            }
            AssetTextExpect::Fail => assert!(asset.is_err()),
        }

        Ok(())
    }
}
