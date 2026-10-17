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
        // Do this before downloading so that an install path we already know we can't write to
        // fails right away, rather than after waiting for a download.
        self.installer.check_install_path()?;

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
        req_builder = self
            .forge
            .maybe_add_token_header_for_download(&asset.url, req_builder)?;
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
    use crate::UbiBuilder;
    use mockito::{Matcher, Mock, Server, ServerGuard};
    use platforms::Platform;
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

    const TOKEN: &str = "not-a-real-token";
    const ASSET_NAME: &str = "project-Linux-x86_64.tar.gz";

    #[test_log::test(tokio::test)]
    async fn token_is_sent_to_a_release_asset_on_the_forge_host() -> Result<()> {
        let mut forge = Server::new_async().await;
        let path = "/test/project/releases/download/v1.0.0/project-Linux-x86_64.tar.gz";
        let asset_url = Url::parse(&forge.url())?.join(path)?;
        let release = mock_latest_release(&mut forge, &asset_url).await;
        let (with_token, without_token) = mock_download(&mut forge, path, 1, 0).await;

        let forge_url = forge.url();
        let mut ubi = UbiBuilder::new()
            .project("test/project")
            .platform(linux_platform())
            .api_base_url(&forge_url)
            .token(TOKEN)
            .build()?;
        let asset = ubi.asset().await?;
        ubi.download_asset(&ubi.reqwest_client, asset).await?;

        release.assert_async().await;
        with_token.assert_async().await;
        without_token.assert_async().await;

        Ok(())
    }

    #[test_log::test(tokio::test)]
    async fn token_is_not_sent_to_a_release_asset_on_another_host() -> Result<()> {
        let mut forge = Server::new_async().await;
        let mut other = Server::new_async().await;
        let path = "/files/project-Linux-x86_64.tar.gz";
        let asset_url = url_on_another_host(&other, path)?;
        let release = mock_latest_release(&mut forge, &asset_url).await;
        let (with_token, without_token) = mock_download(&mut other, path, 0, 1).await;

        let forge_url = forge.url();
        let mut ubi = UbiBuilder::new()
            .project("test/project")
            .platform(linux_platform())
            .api_base_url(&forge_url)
            .token(TOKEN)
            .build()?;
        let asset = ubi.asset().await?;
        ubi.download_asset(&ubi.reqwest_client, asset).await?;

        release.assert_async().await;
        with_token.assert_async().await;
        without_token.assert_async().await;

        Ok(())
    }

    #[test_log::test(tokio::test)]
    async fn token_is_not_sent_to_a_url_on_another_host() -> Result<()> {
        let mut other = Server::new_async().await;
        let path = "/files/tools/project-linux-amd64";
        let url = url_on_another_host(&other, path)?;
        let (with_token, without_token) = mock_download(&mut other, path, 0, 1).await;

        let mut ubi = UbiBuilder::new()
            .url(url.as_str())
            .platform(linux_platform())
            .token(TOKEN)
            .build()?;
        let asset = ubi.asset().await?;
        ubi.download_asset(&ubi.reqwest_client, asset).await?;

        with_token.assert_async().await;
        without_token.assert_async().await;

        Ok(())
    }

    // Makes one mock for a download that carries the token and one for a download that does not,
    // each expecting the given number of requests. Both return a 200, so a test fails on the
    // request count, which says what went wrong, and not on a download error.
    async fn mock_download(
        server: &mut ServerGuard,
        path: &str,
        expect_with_token: usize,
        expect_without_token: usize,
    ) -> (Mock, Mock) {
        let with_token = server
            .mock("GET", path)
            .match_header("Authorization", Matcher::Exact(format!("Bearer {TOKEN}")))
            .with_status(200)
            .with_body("asset")
            .expect(expect_with_token)
            .create_async()
            .await;
        let without_token = server
            .mock("GET", path)
            .match_header("Authorization", Matcher::Missing)
            .with_status(200)
            .with_body("asset")
            .expect(expect_without_token)
            .create_async()
            .await;
        (with_token, without_token)
    }

    async fn mock_latest_release(server: &mut ServerGuard, asset_url: &Url) -> Mock {
        server
            .mock("GET", "/repos/test/project/releases/latest")
            .match_header("Authorization", Matcher::Exact(format!("Bearer {TOKEN}")))
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "assets": [{ "name": ASSET_NAME, "browser_download_url": asset_url }],
                    "published_at": "2024-01-01T00:00:00Z",
                })
                .to_string(),
            )
            .expect(1)
            .create_async()
            .await
    }

    // Every mock server listens on 127.0.0.1, so two of them differ only in their port. Naming one
    // of them `localhost` makes its host differ too, so these tests do not depend on whether the
    // port counts as part of what makes a download host the same as the forge host.
    fn url_on_another_host(server: &ServerGuard, path: &str) -> Result<Url> {
        let mut url = Url::parse(&server.url())?.join(path)?;
        url.set_host(Some("localhost"))?;
        Ok(url)
    }

    fn linux_platform() -> &'static Platform {
        Platform::find("x86_64-unknown-linux-musl").unwrap()
    }
}
