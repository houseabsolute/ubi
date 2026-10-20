use crate::{extension::Extension, forge::Forge, installer::Installer, picker::AssetPicker};
use anyhow::{anyhow, Context, Result};
use log::debug;
use reqwest::{
    header::{HeaderValue, ACCEPT, CONTENT_DISPOSITION},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
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
        // Some forges, like GitLab, let the asset name be any text, so it can contain `/` or be an
        // absolute path. We only use the last part so the download always goes in the temp dir.
        let Some(file_name) = Path::new(&asset.name).file_name() else {
            return Err(anyhow!(
                "the asset name `{}` does not end with a file name",
                asset.name
            ));
        };

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

        // In URL mode the asset name is just the last part of the URL. For an API asset URL that's
        // an ID with no extension, and we need the extension to know how to unpack the file. The
        // server usually sends the real name, so we use that. We only do this when the URL's name
        // has no extension we know, so a server can't swap the type of a file the user named. In
        // project mode the asset name comes from the release, and it's what the picker matched.
        let server_file_name = if self.asset_url.is_some()
            && !matches!(Extension::from_path(Path::new(file_name)), Ok(Some(_)))
        {
            resp.headers()
                .get(CONTENT_DISPOSITION)
                // Some servers send a raw UTF-8 name, which `to_str` rejects.
                .and_then(|h| std::str::from_utf8(h.as_bytes()).ok())
                .and_then(content_disposition_file_name)
        } else {
            None
        };
        let file_name = match &server_file_name {
            Some(n) => {
                debug!("using the file name `{n}` from the Content-Disposition header");
                std::ffi::OsStr::new(n)
            }
            None => file_name,
        };

        let td = tempdir().context("failed to create temporary directory for download")?;
        let archive_path = td.path().join(file_name);
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

// Returns the last part of the file name in a `Content-Disposition` header, if there is one. We
// prefer `filename*`, which can hold any UTF-8 name, over the plain `filename`.
fn content_disposition_file_name(header: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for param in split_header_params(header).into_iter().skip(1) {
        let Some((key, value)) = param.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "filename" => plain = unquote(value.trim()).and_then(|v| last_path_part(&v)),
            "filename*" => {
                // The value looks like `UTF-8''name%20here`. The part before the `''` is the
                // charset and language.
                extended = value
                    .trim()
                    .split_once("''")
                    .and_then(|(_, encoded)| {
                        percent_encoding::percent_decode_str(encoded)
                            .decode_utf8()
                            .ok()
                    })
                    .and_then(|v| last_path_part(&v));
            }
            _ => (),
        }
    }
    extended.or(plain)
}

// Splits a header on `;`, but not on a `;` inside a quoted string, since a quoted file name can
// contain one.
fn split_header_params(header: &str) -> Vec<&str> {
    let mut params = vec![];
    let mut start = 0;
    let mut in_quotes = false;
    let mut escaped = false;
    for (i, c) in header.char_indices() {
        if escaped {
            escaped = false;
        } else if in_quotes && c == '\\' {
            escaped = true;
        } else if c == '"' {
            in_quotes = !in_quotes;
        } else if c == ';' && !in_quotes {
            params.push(&header[start..i]);
            start = i + 1;
        }
    }
    params.push(&header[start..]);
    params
}

// Returns the value without its quotes and with any `\` escapes removed. A value with an opening
// quote but no closing quote is broken, so we return `None` rather than guess.
fn unquote(value: &str) -> Option<String> {
    let Some(inner) = value.strip_prefix('"') else {
        return Some(value.to_string());
    };
    let inner = inner.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.extend(chars.next());
        } else {
            out.push(c);
        }
    }
    Some(out)
}

// The name comes from the server, so it could have a path in it. We only use the last part so the
// download always goes in the temp dir. This also rejects names like `` and `..`.
fn last_path_part(name: &str) -> Option<String> {
    Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from)
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

    // GitLab release link names are free text, so an asset name can have a `/` in it or be an
    // absolute path. The download must still go into the temp dir.
    #[rstest]
    #[case::has_a_slash("linux/amd64", "amd64")]
    #[case::is_absolute("{outside}/x-linux-amd64", "x-linux-amd64")]
    #[test_log::test(tokio::test)]
    async fn download_stays_in_the_temp_dir(
        #[case] name: &str,
        #[case] expect_file_name: &str,
    ) -> Result<()> {
        let outside = tempfile::tempdir()?;
        let name = name.replace("{outside}", &outside.path().to_string_lossy());

        let mut server = Server::new_async().await;
        let path = "/files/project-linux-amd64";
        let url = Url::parse(&server.url())?.join(path)?;
        let mock = server
            .mock("GET", path)
            .with_status(200)
            .with_body("asset")
            .create_async()
            .await;

        let url_str = url.to_string();
        let ubi = UbiBuilder::new()
            .url(&url_str)
            .platform(linux_platform())
            .build()?;
        let download = ubi
            .download_asset(&ubi.reqwest_client, Asset { name, url })
            .await?;

        mock.assert_async().await;
        // `tempdir` makes the download dir directly under the system temp dir.
        let download_dir = download.archive_path.parent().unwrap();
        assert_eq!(download_dir.parent(), Some(std::env::temp_dir().as_path()));
        assert!(download.archive_path.exists());
        assert_eq!(
            download.archive_path.file_name(),
            Some(std::ffi::OsStr::new(expect_file_name)),
        );
        assert!(!outside.path().join("x-linux-amd64").exists());

        Ok(())
    }

    #[rstest]
    #[case::plain("attachment; filename=tool.tar.gz", Some("tool.tar.gz"))]
    #[case::quoted(r#"attachment; filename="tool.tar.gz""#, Some("tool.tar.gz"))]
    #[case::extended("attachment; filename*=UTF-8''t%C3%B6ol.tar.gz", Some("töol.tar.gz"))]
    #[case::extended_wins(
        "attachment; filename=plain.zip; filename*=UTF-8''ext.zip",
        Some("ext.zip")
    )]
    #[case::key_case("attachment; FileName=tool.zip", Some("tool.zip"))]
    #[case::has_a_path(r#"attachment; filename="../../etc/tool.zip""#, Some("tool.zip"))]
    #[case::semicolon_in_quotes(r#"attachment; filename="a;b.tar.gz""#, Some("a;b.tar.gz"))]
    #[case::escaped_quote(r#"attachment; filename="a\"b.zip""#, Some("a\"b.zip"))]
    #[case::no_closing_quote(r#"attachment; filename="tool.zip"#, None)]
    #[case::bad_extended_falls_back(
        "attachment; filename=tool.zip; filename*=UTF-8''..",
        Some("tool.zip")
    )]
    #[case::empty(r#"attachment; filename="""#, None)]
    #[case::dot_dot("attachment; filename=..", None)]
    #[case::no_file_name("attachment", None)]
    #[case::inline_no_file_name("inline", None)]
    fn content_disposition_file_name(#[case] header: &str, #[case] expect: Option<&str>) {
        assert_eq!(
            super::content_disposition_file_name(header).as_deref(),
            expect
        );
    }

    // An API asset URL ends in an ID, not a file name. The server tells us the real file name, and
    // we need it to know the file's type. When the URL's name already has a type we know, we keep
    // it.
    #[rstest]
    #[case::api_asset_url(
        "/repos/owner/repo/releases/assets/12345",
        "project-linux-amd64.tar.gz"
    )]
    #[case::url_has_an_extension("/files/project-linux-amd64.zip", "project-linux-amd64.zip")]
    #[test_log::test(tokio::test)]
    async fn url_mode_download_uses_the_content_disposition_file_name(
        #[case] path: &str,
        #[case] expect_file_name: &str,
    ) -> Result<()> {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", path)
            .with_status(200)
            .with_header(
                "content-disposition",
                "attachment; filename=project-linux-amd64.tar.gz",
            )
            .with_body("asset")
            .create_async()
            .await;

        let server_url = server.url();
        let url_str = Url::parse(&server_url)?.join(path)?.to_string();
        let mut ubi = UbiBuilder::new()
            .url(&url_str)
            .api_base_url(&server_url)
            .platform(linux_platform())
            .build()?;
        let asset = ubi.asset().await?;
        let download = ubi.download_asset(&ubi.reqwest_client, asset).await?;

        mock.assert_async().await;
        assert_eq!(
            download.archive_path.file_name(),
            Some(std::ffi::OsStr::new(expect_file_name)),
        );

        Ok(())
    }

    #[rstest]
    #[case::dot_dot("foo/..")]
    #[case::dot(".")]
    #[case::root("/")]
    #[test_log::test(tokio::test)]
    async fn download_fails_for_a_name_with_no_file_name(#[case] name: &str) -> Result<()> {
        // The name is checked before any request is made, so this URL is never fetched.
        let url_str = "http://127.0.0.1:1/files/project-linux-amd64";
        let url = Url::parse(url_str)?;
        let ubi = UbiBuilder::new()
            .url(url_str)
            .platform(linux_platform())
            .build()?;
        let err = ubi
            .download_asset(
                &ubi.reqwest_client,
                Asset {
                    name: name.to_string(),
                    url,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("the asset name `{name}` does not end with a file name"),
        );

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
