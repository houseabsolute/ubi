use crate::{
    forge::{Forge, ForgeType},
    github::GitHub,
    gitlab::GitLab,
    installer::Installer,
    picker::AssetPicker,
    release::Asset,
    release::Download,
};
use anyhow::{anyhow, Result};
use log::debug;
use platforms::{Platform, OS};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT},
    Client, StatusCode,
};
use std::{
    env,
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
};
use tempfile::tempdir;
use url::Url;

/// `Ubi` is the core of this library, and is used to download and install a binary. Use the
/// [`UbiBuilder`](crate::UbiBuilder) struct to create a new `Ubi` instance.
#[derive(Debug)]
pub struct Ubi<'a> {
    forge: Box<dyn Forge>,
    asset_url: Option<Url>,
    asset_picker: AssetPicker<'a>,
    installer: Installer,
    reqwest_client: Client,
}

impl<'a> Ubi<'a> {
    /// Create a new Ubi instance.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        project: Option<&str>,
        tag: Option<&str>,
        url: Option<&str>,
        install_dir: Option<PathBuf>,
        matching: Option<&'a str>,
        exe: Option<&str>,
        github_token: Option<&str>,
        gitlab_token: Option<&str>,
        platform: &'a Platform,
        is_musl: bool,
        url_base: Option<&str>,
        forge: Option<ForgeType>,
    ) -> Result<Ubi<'a>> {
        let url = url.map(Url::parse).transpose()?;
        let (project_name, forge) = Self::parse_project_name(project, url.as_ref(), forge)?;
        let exe = Self::exe_name(exe, &project_name, platform);
        let install_path = Self::install_path(install_dir, &exe)?;

        let api_base = url_base.map(Url::parse).transpose()?;

        let asset_fetcher: Box<dyn Forge> = match forge {
            ForgeType::GitHub => Box::new(GitHub::new(
                project_name,
                tag.map(String::from),
                api_base,
                github_token,
            )),
            ForgeType::GitLab => Box::new(GitLab::new(
                project_name,
                tag.map(String::from),
                api_base,
                gitlab_token,
            )),
        };
        Ok(Ubi {
            forge: asset_fetcher,
            asset_url: url,
            asset_picker: AssetPicker::new(matching, platform, is_musl),
            installer: Installer::new(install_path, exe),
            reqwest_client: Self::reqwest_client()?,
        })
    }

    fn parse_project_name(
        project: Option<&str>,
        url: Option<&Url>,
        forge: Option<ForgeType>,
    ) -> Result<(String, ForgeType)> {
        let (parsed, from) = if let Some(project) = project {
            if project.starts_with("http") {
                (Url::parse(project)?, format!("--project {project}"))
            } else {
                let base = forge.unwrap_or_default().url_base();
                (base.join(project)?, format!("--project {project}"))
            }
        } else if let Some(u) = url {
            (u.clone(), format!("--url {u}"))
        } else {
            unreachable!(
                "Did not get a --project or --url argument but that should be checked in main.rs"
            );
        };

        let parts = parsed.path().split('/').collect::<Vec<_>>();
        if parts.len() < 3 || parts[1].is_empty() || parts[2].is_empty() {
            return Err(anyhow!("could not parse org and repo name from {from}"));
        }

        // The first part is an empty string for the leading '/' in the path.
        let (org, proj) = (parts[1], parts[2]);
        debug!("Parsed {from} = {org} / {proj}");

        Ok((
            format!("{org}/{proj}"),
            // If the forge argument was not `None` this is kind of pointless, but it should never
            // be _wrong_ in that case.
            ForgeType::from_url(&parsed),
        ))
    }

    fn exe_name(exe: Option<&str>, project_name: &str, platform: &Platform) -> String {
        let name = if let Some(e) = exe {
            match platform.target_os {
                OS::Windows => format!("{e}.exe"),
                _ => e.to_string(),
            }
        } else {
            let parts = project_name.split('/').collect::<Vec<&str>>();
            let e = parts[parts.len() - 1].to_string();
            if matches!(platform.target_os, OS::Windows) {
                format!("{e}.exe")
            } else {
                e
            }
        };
        debug!("exe name = {name}");
        name
    }

    fn install_path(install_dir: Option<PathBuf>, exe: &str) -> Result<PathBuf> {
        let mut path = if let Some(i) = install_dir {
            i
        } else {
            let mut i = env::current_dir()?;
            i.push("bin");
            i
        };
        create_dir_all(&path)?;
        path.push(exe);
        debug!("install path = {}", path.to_string_lossy());
        Ok(path)
    }

    fn reqwest_client() -> Result<Client> {
        let builder = Client::builder().gzip(true);

        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&format!("ubi version {}", super::VERSION))?,
        );
        Ok(builder.default_headers(headers).build()?)
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
    /// * Network errors on requests to GitHub.
    /// * You've reached the API limits for GitHub (try setting the `GITHUB_TOKEN` env var to
    ///   increase these).
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
            return Ok(Asset {
                name: url.path().split('/').last().unwrap().to_string(),
                url: url.clone(),
            });
        }

        let assets = self.forge.fetch_assets(&self.reqwest_client).await?;
        let asset = self.asset_picker.pick_asset(assets)?;
        debug!("picked asset named {}", asset.name);
        Ok(asset)
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
        let mut archive_path = td.path().to_path_buf();
        archive_path.push(&asset.name);
        debug!("archive path is {}", archive_path.to_string_lossy());

        {
            let mut downloaded_file = File::create(&archive_path)?;
            while let Some(c) = resp.chunk().await? {
                downloaded_file.write_all(c.as_ref())?;
            }
        }

        Ok(Download {
            _temp_dir: td,
            archive_path,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use platforms::PlatformReq;
    use std::str::FromStr;

    #[test]
    fn parse_project_name() -> Result<()> {
        let org_and_repo = "some-owner/some-repo";

        let projects = &[
            org_and_repo.to_string(),
            format!("https://github.com/{org_and_repo}"),
            format!("https://github.com/{org_and_repo}/releases"),
            format!("https://github.com/{org_and_repo}/actions/runs/4275745616"),
        ];
        for p in projects {
            let (project_name, forge_type) = Ubi::parse_project_name(Some(p), None, None)?;
            assert_eq!(
                project_name, org_and_repo,
                "got the right project from --project {p}",
            );
            assert_eq!(forge_type, ForgeType::GitHub);

            let (project_name, forge_type) =
                Ubi::parse_project_name(Some(p), None, Some(ForgeType::GitHub))?;
            assert_eq!(
                project_name, org_and_repo,
                "got the right project from --project {p}",
            );
            assert_eq!(forge_type, ForgeType::GitHub);
        }

        {
            let url = Url::parse("https://github.com/houseabsolute/precious/releases/download/v0.1.7/precious-Linux-x86_64-musl.tar.gz")?;
            let (project_name, forge_type) = Ubi::parse_project_name(None, Some(&url), None)?;
            assert_eq!(
                project_name, "houseabsolute/precious",
                "got the right project from the --url",
            );
            assert_eq!(forge_type, ForgeType::GitHub);

            let (project_name, forge_type) =
                Ubi::parse_project_name(None, Some(&url), Some(ForgeType::GitHub))?;
            assert_eq!(
                project_name, "houseabsolute/precious",
                "got the right project from the --url",
            );
            assert_eq!(forge_type, ForgeType::GitHub);
        }

        Ok(())
    }

    #[test]
    fn exe_name() -> Result<()> {
        struct Test {
            exe: Option<&'static str>,
            project_name: &'static str,
            platform: &'static str,
            expect: &'static str,
        }
        let tests: &[Test] = &[
            Test {
                exe: None,
                project_name: "houseabsolute/precious",
                platform: "x86_64-unknown-linux-musl",
                expect: "precious",
            },
            Test {
                exe: None,
                project_name: "houseabsolute/precious",
                platform: "thumbv7m-none-eabi",
                expect: "precious",
            },
            Test {
                exe: None,
                project_name: "houseabsolute/precious",
                platform: "x86_64-apple-darwin",
                expect: "precious",
            },
            Test {
                exe: None,
                project_name: "houseabsolute/precious",
                platform: "x86_64-pc-windows-msvc",
                expect: "precious.exe",
            },
            Test {
                exe: Some("foo"),
                project_name: "houseabsolute/precious",
                platform: "x86_64-pc-windows-msvc",
                expect: "foo.exe",
            },
        ];

        for t in tests {
            let req = PlatformReq::from_str(t.platform)?;
            let platform = req.matching_platforms().next().unwrap();
            assert_eq!(Ubi::exe_name(t.exe, t.project_name, platform), t.expect);
        }

        Ok(())
    }
}
