use crate::{
    extension::Extension, fetcher::GitHubAssetFetcher, picker::AssetPicker, release::Asset,
};
use anyhow::{anyhow, Context, Result};
use binstall_tar::Archive;
use bzip2::read::BzDecoder;
use fern::{
    colors::{Color, ColoredLevelConfig},
    Dispatch,
};
use flate2::read::GzDecoder;
use log::{debug, info};
use platforms::{Platform, OS};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT},
    Client, StatusCode,
};
use result::OptionResultExt;
use std::{
    env,
    fs::{create_dir_all, File},
    io::prelude::*,
    path::{Path, PathBuf},
};
use tempfile::{tempdir, TempDir};
use url::Url;
use xz::read::XzDecoder;
use zip::ZipArchive;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(target_family = "unix")]
use std::fs::{set_permissions, Permissions};
#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug)]
pub(crate) struct Ubi<'a> {
    asset_fetcher: GitHubAssetFetcher,
    exe: String,
    asset_picker: AssetPicker<'a>,
    install_path: PathBuf,
    reqwest_client: Client,
}

#[derive(Debug)]
struct Download {
    // We need to keep the temp dir around so that it's not deleted before
    // we're done with it.
    _temp_dir: TempDir,
    archive_path: PathBuf,
}

impl<'a> Ubi<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        project: Option<&str>,
        tag: Option<&str>,
        url: Option<&str>,
        install_dir: Option<&str>,
        matching: Option<String>,
        exe: Option<&str>,
        platform: &'a Platform,
        github_api_base: Option<String>,
    ) -> Result<Ubi<'a>> {
        let url = url.map(Url::parse).invert()?;
        let project_name = Self::parse_project_name(project, url.as_ref())?;
        let exe = Self::exe_name(exe, &project_name, platform);
        let install_path = Self::install_path(install_dir, &exe)?;
        Ok(Ubi {
            asset_fetcher: GitHubAssetFetcher::new(
                project_name,
                tag.map(|s| s.to_string()),
                url,
                github_api_base,
            ),
            exe,
            asset_picker: AssetPicker::new(matching, platform),
            install_path,
            reqwest_client: Self::reqwest_client()?,
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
        let exe = match exe {
            Some(e) => match platform.target_os {
                OS::Windows => format!("{e}.exe"),
                _ => e.to_string(),
            },
            None => {
                let parts = project_name.split('/').collect::<Vec<&str>>();
                let e = parts[parts.len() - 1].to_string();
                if matches!(platform.target_os, OS::Windows) {
                    format!("{e}.exe")
                } else {
                    e
                }
            }
        };
        debug!("exe name = {exe}");
        exe
    }

    fn install_path(install_dir: Option<&str>, exe: &str) -> Result<PathBuf> {
        let mut i = match install_dir {
            Some(i) => PathBuf::from(i),
            None => {
                let mut i = env::current_dir()?;
                i.push("bin");
                i
            }
        };
        create_dir_all(&i)?;
        i.push(exe);
        debug!("install path = {}", i.to_string_lossy());
        Ok(i)
    }

    fn reqwest_client() -> Result<Client> {
        let builder = Client::builder().gzip(true);

        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&format!("ubi version {VERSION}"))?,
        );

        if let Some(token) = Self::github_token() {
            debug!("adding GitHub token to GitHub requests");
            let bearer = format!("Bearer {token}");
            let mut auth_val = HeaderValue::from_str(&bearer)?;
            auth_val.set_sensitive(true);
            headers.insert(AUTHORIZATION, auth_val);
        }

        Ok(builder.default_headers(headers).build()?)
    }

    fn github_token() -> Option<String> {
        env::var("GITHUB_TOKEN").ok()
    }

    pub(crate) async fn run(&self) -> Result<()> {
        let asset = self.asset().await?;
        let download = self.download_asset(&self.reqwest_client, asset).await?;
        self.install_binary(download).await
    }

    async fn asset(&self) -> Result<Asset> {
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
        let mut res = self.reqwest_client.execute(req).await?;
        if res.status() != StatusCode::OK {
            let mut msg = format!("error requesting {}: {}", asset.url, res.status());
            if let Ok(t) = res.text().await {
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
            while let Some(c) = res.chunk().await? {
                downloaded_file.write_all(c.as_ref())?;
            }
        }

        Ok(Download {
            _temp_dir: td,
            archive_path,
        })
    }

    async fn install_binary(&self, download: Download) -> Result<()> {
        self.extract_binary(download.archive_path)?;
        self.make_binary_executable()?;
        info!("Installed binary into {}", self.install_path.display());

        Ok(())
    }

    fn extract_binary(&self, downloaded_file: PathBuf) -> Result<()> {
        let filename = downloaded_file
            .file_name()
            .unwrap_or_else(|| {
                panic!(
                    "downloaded file path {} has no filename!",
                    downloaded_file.to_string_lossy()
                )
            })
            .to_string_lossy();
        match Extension::from_path(filename) {
            Some(Extension::TarBz)
            | Some(Extension::TarGz)
            | Some(Extension::TarXz)
            | Some(Extension::Tbz)
            | Some(Extension::Tgz)
            | Some(Extension::Txz) => self.extract_tarball(downloaded_file),
            Some(Extension::Bz) => self.unbzip(downloaded_file),
            Some(Extension::Gz) => self.ungzip(downloaded_file),
            Some(Extension::Xz) => self.unxz(downloaded_file),
            Some(Extension::Zip) => self.extract_zip(downloaded_file),
            Some(Extension::Exe) | None => self.copy_executable(downloaded_file),
        }
    }

    fn extract_zip(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("extracting binary from zip file");

        let mut zip = ZipArchive::new(open_file(&downloaded_file)?)?;
        for i in 0..zip.len() {
            let mut zf = zip.by_index(i)?;
            let path = PathBuf::from(zf.name());
            if path.ends_with(&self.exe) {
                let mut buffer: Vec<u8> = Vec::with_capacity(zf.size() as usize);
                zf.read_to_end(&mut buffer)?;
                let mut file = File::create(&self.install_path)?;
                return file.write_all(&buffer).map_err(|e| e.into());
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded zip file",
            self.exe,
        ))
    }

    fn extract_tarball(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!(
            "extracting binary from tarball at {}",
            downloaded_file.to_string_lossy(),
        );

        let mut arch = tar_reader_for(downloaded_file)?;
        for entry in arch.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            debug!("found tarball entry with path {}", path.to_string_lossy());
            if let Some(os_name) = path.file_name() {
                if let Some(n) = os_name.to_str() {
                    if n == self.exe {
                        debug!(
                            "extracting tarball entry to {}",
                            self.install_path.to_string_lossy(),
                        );
                        return match entry.unpack(&self.install_path) {
                            Ok(_) => Ok(()),
                            Err(e) => Err(anyhow::Error::new(e)),
                        };
                    }
                }
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded tarball",
            self.exe,
        ))
    }

    fn unbzip(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("uncompressing binary from bzip file");
        let reader = BzDecoder::new(open_file(&downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn ungzip(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("uncompressing binary from gzip file");
        let reader = GzDecoder::new(open_file(&downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn unxz(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("uncompressing binary from xz file");
        let reader = XzDecoder::new(open_file(&downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn write_to_install_path(&self, mut reader: impl Read) -> Result<()> {
        let mut writer = File::create(&self.install_path)
            .with_context(|| format!("Cannot write to {}", self.install_path.to_string_lossy()))?;
        std::io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    fn copy_executable(&self, exe_file: PathBuf) -> Result<()> {
        debug!("copying binary to final location");
        std::fs::copy(exe_file, &self.install_path)?;

        Ok(())
    }

    fn make_binary_executable(&self) -> Result<()> {
        #[cfg(target_family = "windows")]
        return Ok(());

        #[cfg(target_family = "unix")]
        match set_permissions(&self.install_path, Permissions::from_mode(0o755)) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }
}

fn tar_reader_for(downloaded_file: PathBuf) -> Result<Archive<Box<dyn Read>>> {
    let file = open_file(&downloaded_file)?;

    let ext = downloaded_file.extension();
    match ext {
        Some(ext) => match ext.to_str() {
            Some("bz") | Some("tbz") => Ok(Archive::new(Box::new(BzDecoder::new(file)))),
            Some("gz") | Some("tgz") => Ok(Archive::new(Box::new(GzDecoder::new(file)))),
            Some("xz") | Some("txz") => Ok(Archive::new(Box::new(XzDecoder::new(file)))),
            Some(e) => Err(anyhow!(
                "don't know how to uncompress a tarball with extension = {}",
                e,
            )),
            None => Err(anyhow!(
                "tarball {:?} has a non-UTF-8 extension",
                downloaded_file,
            )),
        },
        None => Ok(Archive::new(Box::new(file))),
    }
}

fn open_file(path: &Path) -> Result<File> {
    File::open(path).with_context(|| format!("Failed to open file at {}", path.to_string_lossy()))
}

pub(crate) fn init_logger(level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
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
    fn extract_binary() -> Result<()> {
        let td = tempdir()?;
        let td_path = td.path().to_string_lossy().to_string();
        let req = PlatformReq::from_str("x86_64-unknown-linux-musl")?;
        let platform = req.matching_platforms().next().unwrap();
        let ubi = Ubi::new(
            Some("org/project"),
            None,
            None,
            Some(&td_path),
            None,
            None,
            platform,
            None,
        )?;
        ubi.extract_binary(PathBuf::from("test-data/project.tar.gz"))?;

        let mut extracted_path = td.path().to_path_buf();
        extracted_path.push("project");
        assert!(extracted_path.exists());
        assert!(extracted_path.is_file());

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

    #[tokio::test]
    async fn asset_picking() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;

        struct Test {
            platforms: &'static [&'static str],
            expect_ubi: Option<(u32, &'static str)>,
            expect_omegasort: Option<(u32, &'static str)>,
        }
        let tests: &[Test] = &[
            Test {
                platforms: &["aarch64-apple-darwin"],
                expect_ubi: Some((96252654, "ubi-Darwin-aarch64.tar.gz")),
                expect_omegasort: Some((84376701, "omegasort_0.0.7_Darwin_arm64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-apple-darwin"],
                expect_ubi: Some((96252671, "ubi-Darwin-x86_64.tar.gz")),
                expect_omegasort: Some((84376694, "omegasort_0.0.7_Darwin_x86_64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-unknown-freebsd"],
                expect_ubi: Some((1, "ubi-FreeBSD-x86_64.tar.gz")),
                expect_omegasort: Some((84376692, "omegasort_0.0.7_FreeBSD_x86_64.tar.gz")),
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
                expect_ubi: Some((96252412, "ubi-Linux-aarch64-musl.tar.gz")),
                expect_omegasort: Some((84376697, "omegasort_0.0.7_Linux_arm64.tar.gz")),
            },
            Test {
                platforms: &["arm-unknown-linux-musleabi"],
                expect_ubi: Some((96252419, "ubi-Linux-arm-musl.tar.gz")),
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
                expect_ubi: Some((96297448, "ubi-Linux-x86_64-musl.tar.gz")),
                expect_omegasort: Some((84376700, "omegasort_0.0.7_Linux_x86_64.tar.gz")),
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
                expect_omegasort: Some((84376695, "omegasort_0.0.7_Windows_arm64.tar.gz")),
            },
            Test {
                platforms: &["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"],
                expect_ubi: Some((96252617, "ubi-Windows-x86_64.zip")),
                expect_omegasort: Some((84376693, "omegasort_0.0.7_Windows_x86_64.tar.gz")),
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
                    let ubi = Ubi::new(
                        Some("houseabsolute/ubi"),
                        None,
                        None,
                        None,
                        None,
                        None,
                        platform,
                        Some(server.url()),
                    )?;
                    let asset = ubi.asset().await?;
                    let expect_ubi_url = Url::parse(&format!(
                        "https://api.github.com/repos/houseabsolute/ubi/releases/assets/{}",
                        expect_ubi.0
                    ))?;
                    assert_eq!(
                        asset.url, expect_ubi_url,
                        "picked {} as ubi url",
                        expect_ubi_url,
                    );
                    assert_eq!(
                        asset.name, expect_ubi.1,
                        "picked {} as ubi asset name",
                        expect_ubi.1,
                    );
                }

                if let Some(expect_omegasort) = t.expect_omegasort {
                    let ubi = Ubi::new(
                        Some("houseabsolute/omegasort"),
                        None,
                        None,
                        None,
                        None,
                        None,
                        platform,
                        Some(server.url()),
                    )?;
                    let asset = ubi.asset().await?;
                    let expect_omegasort_url = Url::parse(&format!(
                        "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/{}",
                        expect_omegasort.0
                    ))?;
                    assert_eq!(
                        asset.url, expect_omegasort_url,
                        "picked {} as omegasort url",
                        expect_omegasort_url,
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

    #[tokio::test]
    // The protobuf repo has some odd release naming. This tests that the
    // matcher handles it.
    async fn matching_unusual_names() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;

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
                let ubi = Ubi::new(
                    Some("protocolbuffers/protobuf"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    platform,
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
    #[tokio::test]
    async fn mkcert_matching() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;

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
                let ubi = Ubi::new(
                    Some("FiloSottile/mkcert"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    platform,
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
    #[tokio::test]
    async fn jq_matching() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;

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
                let ubi = Ubi::new(
                    Some("stedolan/jq"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    platform,
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

    #[tokio::test]
    async fn multiple_matches() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;

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
            let ubi = Ubi::new(
                Some("test/multiple-matches"),
                None,
                None,
                None,
                None,
                None,
                platform,
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

    #[tokio::test]
    async fn macos_arm() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;
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
        let ubi = Ubi::new(
            Some("test/macos"),
            None,
            None,
            None,
            None,
            None,
            platform,
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

    #[tokio::test]
    async fn os_without_arch() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;

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
            let ubi = Ubi::new(
                Some("test/os-without-arch"),
                None,
                None,
                None,
                None,
                None,
                platform,
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
            let ubi = Ubi::new(
                Some("test/os-without-arch"),
                None,
                None,
                None,
                None,
                None,
                platform,
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
