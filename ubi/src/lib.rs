//! A library for downloading and installing pre-built binaries from GitHub.
//!
//! UBI stands for "Universal Binary Installer". It downloads and installs pre-built binaries from
//! GitHub releases. It is designed to be used in shell scripts and other automation.
//!
//! This project also ships a CLI tool named `ubi`. See [the project's GitHub
//! repo](https://github.com/houseabsolute/ubi) for more details on installing and using this tool.
//!
//! ```ignore
//! use ubi::UbiBuilder;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let ubi = UbiBuilder::new()
//!         .project("houseabsolute/precious")
//!         .install_dir("/usr/local/bin")
//!         .build()?;
//!
//!     ubi.install_binary().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! This crate offers several features to control the TLS dependency used by `reqwest`:
//!
#![doc = document_features::document_features!()]

mod arch;
mod extension;
mod fetcher;
mod installer;
mod os;
mod picker;
mod release;

use crate::{
    fetcher::GitHubAssetFetcher, installer::Installer, picker::AssetPicker, release::Asset,
    release::Download,
};
use anyhow::{anyhow, Result};
use log::debug;
use platforms::{Platform, PlatformReq, OS};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT},
    Client, StatusCode,
};
use std::{
    env,
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
    str::FromStr,
};
use tempfile::tempdir;
use url::Url;
use which::which;

// The version of the `ubi` crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `UbiBuilder` is used to create a [`Ubi`] instance.
#[derive(Debug, Default)]
pub struct UbiBuilder<'a> {
    project: Option<&'a str>,
    tag: Option<&'a str>,
    url: Option<&'a str>,
    install_dir: Option<PathBuf>,
    matching: Option<&'a str>,
    exe: Option<&'a str>,
    github_token: Option<&'a str>,
    platform: Option<&'a Platform>,
    github_api_url_base: Option<String>,
}

impl<'a> UbiBuilder<'a> {
    /// Returns a new empty `UbiBuilder`.
    #[must_use]
    pub fn new() -> Self {
        UbiBuilder::default()
    }

    /// Set the project to download from. This can either be just the org/name, like
    /// `houseabsolute/precious`, or the complete GitHub URL to the project, like
    /// `https://github.com/houseabsolute/precious`. It also accepts a URL to any page in the
    /// project, like `https://github.com/houseabsolute/precious/releases`.
    ///
    /// You must set this or set `url`, but not both.
    #[must_use]
    pub fn project(mut self, project: &'a str) -> Self {
        self.project = Some(project);
        self
    }

    /// Set the tag to download. By default the most recent release is downloaded. You cannot set
    /// this with the `url` option.
    #[must_use]
    pub fn tag(mut self, tag: &'a str) -> Self {
        self.tag = Some(tag);
        self
    }

    /// Set the URL to download from. This can be provided instead of a project or tag. This will
    /// not use the GitHub API, so you will never hit the GitHub API limits. That in turn means you
    /// won't have to set a `GITHUB_TOKEN` env var except when downloading a release from a private
    /// repo when the URL is set.
    ///
    /// You must set this or set `project`, but not both.
    #[must_use]
    pub fn url(mut self, url: &'a str) -> Self {
        self.url = Some(url);
        self
    }

    /// Set the directory to install the binary in. If not set, it will default to `./bin`.
    #[must_use]
    pub fn install_dir(mut self, install_dir: PathBuf) -> Self {
        self.install_dir = Some(install_dir);
        self
    }

    /// Set a string to match against the release filename when there are multiple files for your
    /// OS/arch, i.e. "gnu" or "musl". Note that this is only used when there is more than one
    /// matching release filename for your OS/arch. If only one release asset matches your OS/arch,
    /// then this will be ignored.
    #[must_use]
    pub fn matching(mut self, matching: &'a str) -> Self {
        self.matching = Some(matching);
        self
    }

    /// Set the name of the executable to look for in archive files. By default this is the same as
    /// the project name, so for `houseabsolute/precious` we look for `precious` or
    /// `precious.exe`. When running on Windows the ".exe" suffix will be added as needed.
    #[must_use]
    pub fn exe(mut self, exe: &'a str) -> Self {
        self.exe = Some(exe);
        self
    }

    /// Set a GitHub token to use for API requests. If this is not set then this will be taken from
    /// the `GITHUB_TOKEN` env var if it is set.
    #[must_use]
    pub fn github_token(mut self, token: &'a str) -> Self {
        self.github_token = Some(token);
        self
    }

    /// Set the platform to download for. If not set it will be determined based on the current
    /// platform's OS/arch.
    #[must_use]
    pub fn platform(mut self, platform: &'a Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Set the base URL for the GitHub API. This is useful for testing or if you want to operate
    /// against a GitHub Enterprise installation.
    #[must_use]
    pub fn github_api_url_base(mut self, github_api_url_base: String) -> Self {
        self.github_api_url_base = Some(github_api_url_base);
        self
    }

    const TARGET: &'static str = env!("TARGET");

    /// Builds a new [`Ubi`] instance and returns it.
    ///
    /// # Errors
    ///
    /// If you have tried to set incompatible options (setting a `project` or `tag` with a `url`) or
    /// you have not set required options (one of `project` or `url`), then this method will return
    /// an error.
    pub fn build(self) -> Result<Ubi<'a>> {
        if self.project.is_none() && self.url.is_none() {
            return Err(anyhow!("You must set a project or url"));
        }
        if self.url.is_some() && (self.project.is_some() || self.tag.is_some()) {
            return Err(anyhow!("You cannot set a url with a project or tag"));
        }

        let platform = if let Some(p) = self.platform {
            p
        } else {
            let req = PlatformReq::from_str(Self::TARGET)?;
            Platform::ALL
                .iter()
                .find(|p| req.matches(p))
                .ok_or(anyhow!(
                    "Could not find any platform matching {}",
                    Self::TARGET
                ))?
        };

        Ubi::new(
            self.project,
            self.tag,
            self.url,
            self.install_dir,
            self.matching,
            self.exe,
            self.github_token,
            platform,
            platform_is_musl(platform),
            self.github_api_url_base,
        )
    }
}

fn platform_is_musl(platform: &Platform) -> bool {
    if platform.target_os != OS::Linux {
        return false;
    }

    let Ok(ls) = which("ls") else {
        return false;
    };
    let Ok(ldd) = which("ldd") else {
        return false;
    };

    let Ok(output) = std::process::Command::new(ldd).arg(ls).output() else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).contains("musl")
}

/// `Ubi` is the core of this library, and is used to download and install a binary. Use the
/// [`UbiBuilder`] struct to create a new `Ubi` instance.
#[derive(Debug)]
pub struct Ubi<'a> {
    asset_fetcher: GitHubAssetFetcher,
    asset_picker: AssetPicker<'a>,
    installer: Installer,
    reqwest_client: Client,
}

impl<'a> Ubi<'a> {
    /// Create a new Ubi instance.
    #[allow(clippy::too_many_arguments)]
    fn new(
        project: Option<&str>,
        tag: Option<&str>,
        url: Option<&str>,
        install_dir: Option<PathBuf>,
        matching: Option<&'a str>,
        exe: Option<&str>,
        github_token: Option<&str>,
        platform: &'a Platform,
        is_musl: bool,
        github_api_url_base: Option<String>,
    ) -> Result<Ubi<'a>> {
        let url = if let Some(u) = url {
            Some(Url::parse(u)?)
        } else {
            None
        };
        let project_name = Self::parse_project_name(project, url.as_ref())?;
        let exe = Self::exe_name(exe, &project_name, platform);
        let install_path = Self::install_path(install_dir, &exe)?;
        Ok(Ubi {
            asset_fetcher: GitHubAssetFetcher::new(
                project_name,
                tag.map(std::string::ToString::to_string),
                url,
                github_api_url_base,
            ),
            asset_picker: AssetPicker::new(matching, platform, is_musl),
            installer: Installer::new(install_path, exe),
            reqwest_client: Self::reqwest_client(github_token)?,
        })
    }

    fn parse_project_name(project: Option<&str>, url: Option<&Url>) -> Result<String> {
        let (parsed, from) = if let Some(project) = project {
            if project.starts_with("http") {
                (Url::parse(project)?, format!("--project {project}"))
            } else {
                let base = Url::parse("https://github.com")?;
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

        Ok(format!("{org}/{proj}"))
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

    fn reqwest_client(github_token: Option<&str>) -> Result<Client> {
        let builder = Client::builder().gzip(true);

        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&format!("ubi version {VERSION}"))?,
        );

        let mut github_token = github_token.map(String::from);
        if github_token.is_none() {
            github_token = env::var("GITHUB_TOKEN").ok();
        }

        if let Some(token) = github_token {
            debug!("adding GitHub token to GitHub requests");
            let bearer = format!("Bearer {token}");
            let mut auth_val = HeaderValue::from_str(&bearer)?;
            auth_val.set_sensitive(true);
            headers.insert(AUTHORIZATION, auth_val);
        }

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

    async fn asset(&mut self) -> Result<Asset> {
        let assets = self
            .asset_fetcher
            .fetch_assets(&self.reqwest_client)
            .await?;
        let asset = self.asset_picker.pick_asset(assets)?;
        debug!("picked asset named {}", asset.name);
        Ok(asset)
    }

    async fn download_asset(&self, client: &Client, asset: Asset) -> Result<Download> {
        debug!("downloading asset from {}", asset.url);

        let req = client
            .get(asset.url.clone())
            .header(ACCEPT, HeaderValue::from_str("application/octet-stream")?)
            .build()?;
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

#[cfg(feature = "logging")]
use fern::{
    colors::{Color, ColoredLevelConfig},
    Dispatch,
};

/// This function initializes logging for the application. It's public for the sake of the `ubi`
/// binary, but it lives in the library crate so that test code can also enable logging.
///
/// # Errors
///
/// This can return a `log::SetLoggerError` error.
#[cfg(feature = "logging")]
pub fn init_logger(level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
    let line_colors = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::BrightBlack)
        .debug(Color::BrightBlack)
        .trace(Color::BrightBlack);
    let level_colors = line_colors.info(Color::Green).debug(Color::Black);

    Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{color_line}[{target}][{level}{color_line}] {message}\x1B[0m",
                color_line = format_args!(
                    "\x1B[{}m",
                    line_colors.get_color(&record.level()).to_fg_str()
                ),
                target = record.target(),
                level = level_colors.color(record.level()),
                message = message,
            ));
        })
        .level(level)
        // This is very noisy.
        .level_for("hyper", log::LevelFilter::Error)
        .chain(std::io::stderr())
        .apply()
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use mockito::Server;
    use platforms::PlatformReq;
    use reqwest::header::ACCEPT;
    use std::str::FromStr;
    use test_log::test;

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
            let project_name = Ubi::parse_project_name(Some(p), None)?;
            assert_eq!(
                project_name, org_and_repo,
                "got the right project from --project {p}",
            );
        }

        {
            let url = Url::parse("https://github.com/houseabsolute/precious/releases/download/v0.1.7/precious-Linux-x86_64-musl.tar.gz")?;
            let project_name = Ubi::parse_project_name(None, Some(&url))?;
            assert_eq!(
                project_name, "houseabsolute/precious",
                "got the right project from the --url",
            );
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

    #[test(tokio::test)]
    #[allow(clippy::too_many_lines)]
    async fn asset_picking() -> Result<()> {
        struct Test {
            platforms: &'static [&'static str],
            expect_ubi: Option<(u32, &'static str)>,
            expect_omegasort: Option<(u32, &'static str)>,
        }
        let tests: &[Test] = &[
            Test {
                platforms: &["aarch64-apple-darwin"],
                expect_ubi: Some((96_252_654, "ubi-Darwin-aarch64.tar.gz")),
                expect_omegasort: Some((84_376_701, "omegasort_0.0.7_Darwin_arm64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-apple-darwin"],
                expect_ubi: Some((96_252_671, "ubi-Darwin-x86_64.tar.gz")),
                expect_omegasort: Some((84_376_694, "omegasort_0.0.7_Darwin_x86_64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-unknown-freebsd"],
                expect_ubi: Some((1, "ubi-FreeBSD-x86_64.tar.gz")),
                expect_omegasort: Some((84_376_692, "omegasort_0.0.7_FreeBSD_x86_64.tar.gz")),
            },
            Test {
                platforms: &["aarch64-fuchsia"],
                expect_ubi: Some((2, "ubi-Fuchsia-aarch64.tar.gz")),
                expect_omegasort: Some((2, "omegasort_0.0.7_Fuchsia_arm64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-fuchsia"],
                expect_ubi: Some((3, "ubi-Fuchsia-x86_64.tar.gz")),
                expect_omegasort: Some((3, "omegasort_0.0.7_Fuchsia_x86_64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-unknown-illumos"],
                expect_ubi: Some((4, "ubi-Illumos-x86_64.tar.gz")),
                expect_omegasort: Some((4, "omegasort_0.0.7_Illumos_x86_64.tar.gz")),
            },
            Test {
                platforms: &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"],
                expect_ubi: Some((96_252_412, "ubi-Linux-aarch64-musl.tar.gz")),
                expect_omegasort: Some((84_376_697, "omegasort_0.0.7_Linux_arm64.tar.gz")),
            },
            Test {
                platforms: &["arm-unknown-linux-musleabi"],
                expect_ubi: Some((96_252_419, "ubi-Linux-arm-musl.tar.gz")),
                expect_omegasort: Some((42, "omegasort_0.0.7_Linux_arm.tar.gz")),
            },
            Test {
                platforms: &[
                    "i586-unknown-linux-gnu",
                    "i586-unknown-linux-musl",
                    "i686-unknown-linux-gnu",
                    "i686-unknown-linux-musl",
                ],
                expect_ubi: Some((62, "ubi-Linux-i586-musl.tar.gz")),
                expect_omegasort: Some((62, "omegasort_0.0.7_Linux_386.tar.gz")),
            },
            Test {
                platforms: &["mips-unknown-linux-gnu", "mips-unknown-linux-musl"],
                expect_ubi: Some((50, "ubi-Linux-mips-musl.tar.gz")),
                expect_omegasort: Some((50, "omegasort_0.0.7_Linux_mips.tar.gz")),
            },
            Test {
                platforms: &["mipsel-unknown-linux-gnu", "mipsel-unknown-linux-musl"],
                expect_ubi: Some((52, "ubi-Linux-mipsel-musl.tar.gz")),
                expect_omegasort: Some((52, "omegasort_0.0.7_Linux_mipsle.tar.gz")),
            },
            Test {
                platforms: &[
                    "mips64-unknown-linux-gnuabi64",
                    "mips64-unknown-linux-muslabi64",
                ],
                expect_ubi: Some((51, "ubi-Linux-mips64-musl.tar.gz")),
                expect_omegasort: Some((51, "omegasort_0.0.7_Linux_mips64.tar.gz")),
            },
            Test {
                platforms: &[
                    "mips64el-unknown-linux-gnuabi64",
                    "mips64el-unknown-linux-muslabi64",
                ],
                expect_ubi: Some((53, "ubi-Linux-mips64el-musl.tar.gz")),
                expect_omegasort: Some((53, "omegasort_0.0.7_Linux_mips64le.tar.gz")),
            },
            Test {
                platforms: &["powerpc-unknown-linux-gnu"],
                expect_ubi: Some((54, "ubi-Linux-powerpc-gnu.tar.gz")),
                expect_omegasort: Some((54, "omegasort_0.0.7_Linux_ppc.tar.gz")),
            },
            Test {
                platforms: &["powerpc64-unknown-linux-gnu"],
                expect_ubi: Some((55, "ubi-Linux-powerpc64-gnu.tar.gz")),
                expect_omegasort: Some((55, "omegasort_0.0.7_Linux_ppc64.tar.gz")),
            },
            Test {
                platforms: &["powerpc64le-unknown-linux-gnu"],
                expect_ubi: Some((56, "ubi-Linux-powerpc64le-gnu.tar.gz")),
                expect_omegasort: Some((56, "omegasort_0.0.7_Linux_ppc64le.tar.gz")),
            },
            Test {
                platforms: &["riscv64gc-unknown-linux-gnu"],
                expect_ubi: Some((57, "ubi-Linux-riscv64-gnu.tar.gz")),
                expect_omegasort: Some((57, "omegasort_0.0.7_Linux_riscv64.tar.gz")),
            },
            Test {
                platforms: &["s390x-unknown-linux-gnu"],
                expect_ubi: Some((58, "ubi-Linux-s390x-gnu.tar.gz")),
                expect_omegasort: Some((58, "omegasort_0.0.7_Linux_s390x.tar.gz")),
            },
            Test {
                platforms: &["sparc64-unknown-linux-gnu"],
                expect_ubi: Some((59, "ubi-Linux-sparc64-gnu.tar.gz")),
                expect_omegasort: None,
            },
            Test {
                platforms: &["x86_64-unknown-linux-musl"],
                expect_ubi: Some((96_297_448, "ubi-Linux-x86_64-musl.tar.gz")),
                expect_omegasort: Some((84_376_700, "omegasort_0.0.7_Linux_x86_64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-unknown-netbsd"],
                expect_ubi: Some((5, "ubi-NetBSD-x86_64.tar.gz")),
                expect_omegasort: Some((5, "omegasort_0.0.7_NetBSD_x86_64.tar.gz")),
            },
            Test {
                platforms: &["sparcv9-sun-solaris"],
                expect_ubi: Some((61, "ubi-Solaris-sparc64.tar.gz")),
                expect_omegasort: None,
            },
            Test {
                platforms: &["x86_64-pc-solaris", "x86_64-sun-solaris"],
                expect_ubi: Some((6, "ubi-Solaris-x86_64.tar.gz")),
                expect_omegasort: Some((6, "omegasort_0.0.7_Solaris_x86_64.tar.gz")),
            },
            Test {
                platforms: &["aarch64-pc-windows-msvc"],
                expect_ubi: Some((7, "ubi-Windows-aarch64.zip")),
                expect_omegasort: Some((84_376_695, "omegasort_0.0.7_Windows_arm64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"],
                expect_ubi: Some((96_252_617, "ubi-Windows-x86_64.zip")),
                expect_omegasort: Some((84_376_693, "omegasort_0.0.7_Windows_x86_64.tar.gz")),
            },
        ];

        let mut server = Server::new_async().await;
        let m1 = server
            .mock("GET", "/repos/houseabsolute/ubi/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(UBI_LATEST_RESPONSE)
            .expect_at_least(tests.len())
            .create_async()
            .await;
        let m2 = server
            .mock("GET", "/repos/houseabsolute/omegasort/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(OMEGASORT_LATEST_RESPONSE)
            .expect_at_least(tests.len())
            .create_async()
            .await;

        for t in tests {
            for p in t.platforms {
                let req = PlatformReq::from_str(p)
                    .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
                let platform = req.matching_platforms().next().unwrap();

                if let Some(expect_ubi) = t.expect_ubi {
                    let mut ubi = Ubi::new(
                        Some("houseabsolute/ubi"),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        platform,
                        false,
                        Some(server.url()),
                    )?;
                    let asset = ubi.asset().await?;
                    let expect_ubi_url = Url::parse(&format!(
                        "https://api.github.com/repos/houseabsolute/ubi/releases/assets/{}",
                        expect_ubi.0
                    ))?;
                    assert_eq!(
                        asset.url, expect_ubi_url,
                        "picked {expect_ubi_url} as ubi url",
                    );
                    assert_eq!(
                        asset.name, expect_ubi.1,
                        "picked {} as ubi asset name",
                        expect_ubi.1,
                    );
                }

                if let Some(expect_omegasort) = t.expect_omegasort {
                    let mut ubi = Ubi::new(
                        Some("houseabsolute/omegasort"),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        platform,
                        false,
                        Some(server.url()),
                    )?;
                    let asset = ubi.asset().await?;
                    let expect_omegasort_url = Url::parse(&format!(
                        "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/{}",
                        expect_omegasort.0
                    ))?;
                    assert_eq!(
                        asset.url, expect_omegasort_url,
                        "picked {expect_omegasort_url} as omegasort url",
                    );
                    assert_eq!(
                        asset.name, expect_omegasort.1,
                        "picked {} as omegasort ID",
                        expect_omegasort.1,
                    );
                }
            }
        }

        m1.assert_async().await;
        m2.assert_async().await;

        Ok(())
    }

    // jq '[.assets[] | {"url": .url} + {"name": .name}]' release.json
    const UBI_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252654",
      "name": "ubi-Darwin-aarch64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252671",
      "name": "ubi-Darwin-x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/1",
      "name": "ubi-FreeBSD-x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/2",
      "name": "ubi-Fuchsia-aarch64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/3",
      "name": "ubi-Fuchsia-x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/4",
      "name": "ubi-Illumos-x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252412",
      "name": "ubi-Linux-aarch64-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252419",
      "name": "ubi-Linux-arm-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/62",
      "name": "ubi-Linux-i586-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/50",
      "name": "ubi-Linux-mips-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/52",
      "name": "ubi-Linux-mipsel-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/51",
      "name": "ubi-Linux-mips64-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/53",
      "name": "ubi-Linux-mips64el-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/54",
      "name": "ubi-Linux-powerpc-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/55",
      "name": "ubi-Linux-powerpc64-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/56",
      "name": "ubi-Linux-powerpc64le-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/57",
      "name": "ubi-Linux-riscv64-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/58",
      "name": "ubi-Linux-s390x-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/59",
      "name": "ubi-Linux-sparc64-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96297448",
      "name": "ubi-Linux-x86_64-musl.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/5",
      "name": "ubi-NetBSD-x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/61",
      "name": "ubi-Solaris-sparc64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/6",
      "name": "ubi-Solaris-x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/60",
      "name": "ubi-Solaris-sparcv9.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/7",
      "name": "ubi-Windows-aarch64.zip"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252617",
      "name": "ubi-Windows-x86_64.zip"
    }
  ]
}
"#;

    const OMEGASORT_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376696",
      "name": "checksums.txt"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376701",
      "name": "omegasort_0.0.7_Darwin_arm64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376694",
      "name": "omegasort_0.0.7_Darwin_x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376698",
      "name": "omegasort_0.0.7_FreeBSD_arm64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376699",
      "name": "omegasort_0.0.7_FreeBSD_i386.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376692",
      "name": "omegasort_0.0.7_FreeBSD_x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/2",
      "name": "omegasort_0.0.7_Fuchsia_arm64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/3",
      "name": "omegasort_0.0.7_Fuchsia_x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/4",
      "name": "omegasort_0.0.7_Illumos_x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/42",
      "name": "omegasort_0.0.7_Linux_arm.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376697",
      "name": "omegasort_0.0.7_Linux_arm64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/62",
      "name": "omegasort_0.0.7_Linux_386.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/50",
      "name": "omegasort_0.0.7_Linux_mips.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/52",
      "name": "omegasort_0.0.7_Linux_mipsle.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/51",
      "name": "omegasort_0.0.7_Linux_mips64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/53",
      "name": "omegasort_0.0.7_Linux_mips64le.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/54",
      "name": "omegasort_0.0.7_Linux_ppc.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/55",
      "name": "omegasort_0.0.7_Linux_ppc64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/56",
      "name": "omegasort_0.0.7_Linux_ppc64le.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/57",
      "name": "omegasort_0.0.7_Linux_riscv64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/58",
      "name": "omegasort_0.0.7_Linux_s390x.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376703",
      "name": "omegasort_0.0.7_Linux_i386.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376700",
      "name": "omegasort_0.0.7_Linux_x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/5",
      "name": "omegasort_0.0.7_NetBSD_x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/6",
      "name": "omegasort_0.0.7_Solaris_x86_64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376695",
      "name": "omegasort_0.0.7_Windows_arm64.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376702",
      "name": "omegasort_0.0.7_Windows_i386.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376693",
      "name": "omegasort_0.0.7_Windows_x86_64.tar.gz"
    }
  ]
}
"#;

    #[test(tokio::test)]
    // The protobuf repo has some odd release naming. This tests that the
    // matcher handles it.
    async fn matching_unusual_names() -> Result<()> {
        struct Test {
            platforms: &'static [&'static str],
            expect: &'static str,
        }
        let tests: &[Test] = &[
            Test {
                platforms: &["aarch64-apple-darwin"],
                expect: "protoc-22.2-osx-aarch_64.zip",
            },
            Test {
                platforms: &["x86_64-apple-darwin"],
                expect: "protoc-22.2-osx-x86_64.zip",
            },
            Test {
                platforms: &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"],
                expect: "protoc-22.2-linux-aarch_64.zip",
            },
            Test {
                platforms: &[
                    "i586-unknown-linux-gnu",
                    "i586-unknown-linux-musl",
                    "i686-unknown-linux-gnu",
                    "i686-unknown-linux-musl",
                ],
                expect: "protoc-22.2-linux-x86_32.zip",
            },
            Test {
                platforms: &["powerpc64le-unknown-linux-gnu"],
                expect: "protoc-22.2-linux-ppcle_64.zip",
            },
            Test {
                platforms: &["s390x-unknown-linux-gnu"],
                expect: "protoc-22.2-linux-s390_64.zip",
            },
            Test {
                platforms: &["x86_64-unknown-linux-musl"],
                expect: "protoc-22.2-linux-x86_64.zip",
            },
            Test {
                platforms: &["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"],
                expect: "protoc-22.2-win64.zip",
            },
            Test {
                platforms: &[
                    "i586-pc-windows-msvc",
                    "i686-pc-windows-gnu",
                    "i686-pc-windows-msvc",
                ],
                expect: "protoc-22.2-win32.zip",
            },
        ];

        let mut server = Server::new_async().await;
        let m1 = server
            .mock("GET", "/repos/protocolbuffers/protobuf/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(PROTOBUF_LATEST_RESPONSE)
            .expect_at_least(tests.len())
            .create_async()
            .await;

        for t in tests {
            for p in t.platforms {
                let req = PlatformReq::from_str(p)
                    .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
                let platform = req.matching_platforms().next().unwrap();
                let mut ubi = Ubi::new(
                    Some("protocolbuffers/protobuf"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    platform,
                    false,
                    Some(server.url()),
                )?;
                let asset = ubi.asset().await?;
                assert_eq!(
                    asset.name, t.expect,
                    "picked {} as protobuf asset name",
                    t.expect
                );
            }
        }

        m1.assert_async().await;

        Ok(())
    }

    const PROTOBUF_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875803",
      "name": "protobuf-22.2.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875802",
      "name": "protobuf-22.2.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875801",
      "name": "protoc-22.2-linux-aarch_64.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875800",
      "name": "protoc-22.2-linux-ppcle_64.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875799",
      "name": "protoc-22.2-linux-s390_64.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875810",
      "name": "protoc-22.2-linux-x86_32.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875811",
      "name": "protoc-22.2-linux-x86_64.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875812",
      "name": "protoc-22.2-osx-aarch_64.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875813",
      "name": "protoc-22.2-osx-universal_binary.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875814",
      "name": "protoc-22.2-osx-x86_64.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875815",
      "name": "protoc-22.2-win32.zip"
    },
    {
      "url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875816",
      "name": "protoc-22.2-win64.zip"
    }
  ]
}
"#;

    // Reported in https://github.com/houseabsolute/ubi/issues/34
    #[test(tokio::test)]
    async fn mkcert_matching() -> Result<()> {
        struct Test {
            platforms: &'static [&'static str],
            expect: &'static str,
        }
        let tests: &[Test] = &[
            Test {
                platforms: &["aarch64-apple-darwin"],
                expect: "mkcert-v1.4.4-darwin-arm64",
            },
            Test {
                platforms: &["x86_64-apple-darwin"],
                expect: "mkcert-v1.4.4-darwin-amd64",
            },
            Test {
                platforms: &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"],
                expect: "mkcert-v1.4.4-linux-arm64",
            },
            Test {
                platforms: &["arm-unknown-linux-gnueabi", "arm-unknown-linux-musleabi"],
                expect: "mkcert-v1.4.4-linux-arm",
            },
            Test {
                platforms: &["x86_64-unknown-linux-musl"],
                expect: "mkcert-v1.4.4-linux-amd64",
            },
            Test {
                platforms: &["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"],
                expect: "mkcert-v1.4.4-windows-amd64.exe",
            },
        ];

        let mut server = Server::new_async().await;
        let m1 = server
            .mock("GET", "/repos/FiloSottile/mkcert/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(MKCERT_LATEST_RESPONSE)
            .expect_at_least(tests.len())
            .create_async()
            .await;

        for t in tests {
            for p in t.platforms {
                let req = PlatformReq::from_str(p)
                    .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
                let platform = req.matching_platforms().next().unwrap();
                let mut ubi = Ubi::new(
                    Some("FiloSottile/mkcert"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    platform,
                    false,
                    Some(server.url()),
                )?;
                let asset = ubi.asset().await?;
                assert_eq!(
                    asset.name, t.expect,
                    "picked {} as protobuf asset name",
                    t.expect
                );
            }
        }

        m1.assert_async().await;

        Ok(())
    }

    const MKCERT_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709952",
      "name": "mkcert-v1.4.4-darwin-amd64"
    },
    {
      "url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709954",
      "name": "mkcert-v1.4.4-darwin-arm64"
    },
    {
      "url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709955",
      "name": "mkcert-v1.4.4-linux-amd64"
    },
    {
      "url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709956",
      "name": "mkcert-v1.4.4-linux-arm"
    },
    {
      "url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709957",
      "name": "mkcert-v1.4.4-linux-arm64"
    },
    {
      "url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709958",
      "name": "mkcert-v1.4.4-windows-amd64.exe"
    },
    {
      "url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709963",
      "name": "mkcert-v1.4.4-windows-arm64.exe"
    }
  ]
}"#;

    // Reported in https://github.com/houseabsolute/ubi/issues/34
    #[test(tokio::test)]
    async fn jq_matching() -> Result<()> {
        struct Test {
            platforms: &'static [&'static str],
            expect: &'static str,
        }
        let tests: &[Test] = &[
            Test {
                platforms: &["x86_64-apple-darwin"],
                expect: "jq-osx-amd64",
            },
            Test {
                platforms: &["x86_64-unknown-linux-musl"],
                expect: "jq-linux64",
            },
            Test {
                platforms: &[
                    "i586-pc-windows-msvc",
                    "i686-pc-windows-gnu",
                    "i686-pc-windows-msvc",
                ],
                expect: "jq-win32.exe",
            },
        ];

        let mut server = Server::new_async().await;
        let m1 = server
            .mock("GET", "/repos/stedolan/jq/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(JQ_LATEST_RESPONSE)
            .expect_at_least(tests.len())
            .create_async()
            .await;

        for t in tests {
            for p in t.platforms {
                let req = PlatformReq::from_str(p)
                    .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
                let platform = req.matching_platforms().next().unwrap();
                let mut ubi = Ubi::new(
                    Some("stedolan/jq"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    platform,
                    false,
                    Some(server.url()),
                )?;
                let asset = ubi.asset().await?;
                assert_eq!(
                    asset.name, t.expect,
                    "picked {} as protobuf asset name",
                    t.expect
                );
            }
        }

        m1.assert_async().await;

        Ok(())
    }

    const JQ_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/stedolan/jq/releases/assets/9780532",
      "name": "jq-1.6.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/stedolan/jq/releases/assets/9780533",
      "name": "jq-1.6.zip"
    },
    {
      "url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521004",
      "name": "jq-linux32"
    },
    {
      "url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521005",
      "name": "jq-linux64"
    },
    {
      "url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521006",
      "name": "jq-osx-amd64"
    },
    {
      "url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521007",
      "name": "jq-win32.exe"
    },
    {
      "url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521008",
      "name": "jq-win64.exe"
    }
  ]
}"#;

    #[test(tokio::test)]
    async fn multiple_matches() -> Result<()> {
        let platforms = ["x86_64-pc-windows-gnu", "i686-pc-windows-gnu"];

        let mut server = Server::new_async().await;
        let m1 = server
            .mock("GET", "/repos/test/multiple-matches/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(MULTIPLE_MATCHES_RESPONSE)
            .expect_at_least(platforms.len())
            .create_async()
            .await;

        for p in platforms {
            let req = PlatformReq::from_str(p)
                .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
            let platform = req.matching_platforms().next().unwrap();
            let mut ubi = Ubi::new(
                Some("test/multiple-matches"),
                None,
                None,
                None,
                None,
                None,
                None,
                platform,
                false,
                Some(server.url()),
            )?;
            let asset = ubi.asset().await?;
            let expect = "mm-i686-pc-windows-gnu.zip";
            assert_eq!(asset.name, expect, "picked {expect} as protobuf asset name");
        }

        m1.assert_async().await;

        Ok(())
    }

    const MULTIPLE_MATCHES_RESPONSE: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/test/multiple-matches/releases/assets/9521007",
      "name": "mm-i686-pc-windows-gnu.zip"
    },
    {
      "url": "https://api.github.com/repos/test/multiple-matches/releases/assets/9521008",
      "name": "mm-i686-pc-windows-msvc.zip"
    }
  ]
}"#;

    #[test(tokio::test)]
    async fn macos_arm() -> Result<()> {
        let mut server = Server::new_async().await;
        let m1 = server
            .mock("GET", "/repos/test/macos/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(MACOS_RESPONSE1)
            .expect_at_least(1)
            .create_async()
            .await;

        let p = "aarch64-apple-darwin";
        let req = PlatformReq::from_str(p)
            .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
        let platform = req.matching_platforms().next().unwrap();
        let mut ubi = Ubi::new(
            Some("test/macos"),
            None,
            None,
            None,
            None,
            None,
            None,
            platform,
            false,
            Some(server.url()),
        )?;

        {
            let asset = ubi.asset().await?;
            let expect = "bat-v0.23.0-x86_64-apple-darwin.tar.gz";
            assert_eq!(
                asset.name, expect,
                "picked {expect} as macos bat asset name when only x86 binary is available"
            );
            m1.assert_async().await;
        }

        server.reset();

        let m2 = server
            .mock("GET", "/repos/test/macos/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(MACOS_RESPONSE2)
            .expect_at_least(1)
            .create_async()
            .await;

        {
            let asset = ubi.asset().await?;
            let expect = "bat-v0.23.0-aarch64-apple-darwin.tar.gz";
            assert_eq!(
                asset.name, expect,
                "picked {expect} as macos bat asset name when ARM binary is available"
            );
            m2.assert_async().await;
        }

        Ok(())
    }

    const MACOS_RESPONSE1: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "bat-v0.23.0-i686-unknown-linux-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "bat-v0.23.0-x86_64-apple-darwin.tar.gz"
    }
  ]
}"#;

    const MACOS_RESPONSE2: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "bat-v0.23.0-i686-unknown-linux-gnu.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "bat-v0.23.0-x86_64-apple-darwin.tar.gz"
    },
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "bat-v0.23.0-aarch64-apple-darwin.tar.gz"
    }
  ]
}"#;

    #[test(tokio::test)]
    async fn os_without_arch() -> Result<()> {
        {
            let mut server = Server::new_async().await;
            let m1 = server
                .mock("GET", "/repos/test/os-without-arch/releases/latest")
                .match_header(ACCEPT.as_str(), "application/json")
                .with_status(reqwest::StatusCode::OK.as_u16() as usize)
                .with_body(OS_WITHOUT_ARCH_RESPONSE1)
                .expect_at_least(1)
                .create_async()
                .await;

            let p = "x86_64-apple-darwin";
            let req = PlatformReq::from_str(p)
                .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
            let platform = req.matching_platforms().next().unwrap();
            let mut ubi = Ubi::new(
                Some("test/os-without-arch"),
                None,
                None,
                None,
                None,
                None,
                None,
                platform,
                false,
                Some(server.url()),
            )?;
            let asset = ubi.asset().await?;
            let expect = "gvproxy-darwin";
            assert_eq!(asset.name, expect, "picked {expect} as protobuf asset name");

            m1.assert_async().await;
        }

        {
            let mut server = Server::new_async().await;
            let m1 = server
                .mock("GET", "/repos/test/os-without-arch/releases/latest")
                .match_header(ACCEPT.as_str(), "application/json")
                .with_status(reqwest::StatusCode::OK.as_u16() as usize)
                .with_body(OS_WITHOUT_ARCH_RESPONSE2)
                .expect_at_least(1)
                .create_async()
                .await;

            let p = "x86_64-apple-darwin";
            let req = PlatformReq::from_str(p)
                .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
            let platform = req.matching_platforms().next().unwrap();
            let mut ubi = Ubi::new(
                Some("test/os-without-arch"),
                None,
                None,
                None,
                None,
                None,
                None,
                platform,
                false,
                Some(server.url()),
            )?;
            let asset = ubi.asset().await;
            assert!(
                asset.is_err(),
                "should not have found an asset because the only darwin asset is for arm64",
            );

            m1.assert_async().await;
        }

        Ok(())
    }

    const OS_WITHOUT_ARCH_RESPONSE1: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "gvproxy-darwin"
    },
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "gvproxy-linux-amd64"
    },
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891187",
      "name": "gvproxy-linux-arm64"
    }
  ]
}"#;

    const OS_WITHOUT_ARCH_RESPONSE2: &str = r#"
{
  "assets": [
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "gvproxy-darwin-arm64"
    },
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "gvproxy-linux-amd64"
    },
    {
      "url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891187",
      "name": "gvproxy-linux-arm64"
    }
  ]
}"#;
}
