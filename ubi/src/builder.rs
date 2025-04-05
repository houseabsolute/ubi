/// The `builder` module contains the `UbiBuilder` struct which is used to create a `Ubi` instance.
use crate::{
    forge::{Forge, ForgeType},
    installer::{ArchiveInstaller, ExeInstaller, Installer},
    picker::AssetPicker,
    ubi::Ubi,
};
use anyhow::{anyhow, Result};
use log::debug;
use platforms::{Platform, PlatformReq, OS};
use reqwest::{
    header::{HeaderMap, HeaderValue, USER_AGENT},
    Client,
};
use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
};
use url::Url;
use which::which;

/// `UbiBuilder` is used to create a [`Ubi`] instance.
#[derive(Debug, Default)]
#[allow(clippy::module_name_repetitions)]
pub struct UbiBuilder<'a> {
    project: Option<&'a str>,
    tag: Option<&'a str>,
    url: Option<&'a str>,
    install_dir: Option<PathBuf>,
    matching: Option<&'a str>,
    exe: Option<&'a str>,
    rename_exe_to: Option<&'a str>,
    extract_all: bool,
    token: Option<&'a str>,
    platform: Option<&'a Platform>,
    is_musl: Option<bool>,
    api_base_url: Option<&'a str>,
    forge: Option<ForgeType>,
}

impl<'a> UbiBuilder<'a> {
    /// Returns a new empty `UbiBuilder`.
    #[must_use]
    pub fn new() -> Self {
        UbiBuilder::default()
    }

    /// Set the project to download from. This can either be just the org/name, like
    /// `houseabsolute/precious`, or the complete forge site URL to the project, like
    /// `https://github.com/houseabsolute/precious` or `https://gitlab.com/gitlab-org/cli`. It also
    /// accepts a URL to any page in the project, like
    /// `https://github.com/houseabsolute/precious/releases`.
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

    /// Set the URL to download from. This can be provided instead of a project or tag. This will not
    /// use the forge site API, so you will never hit API limits. That in turn means you won't have
    /// to set a token env var except when downloading a release from a private repo when the URL is
    /// set.
    ///
    /// You must set this or set `project`, but not both.
    #[must_use]
    pub fn url(mut self, url: &'a str) -> Self {
        self.url = Some(url);
        self
    }

    /// Set the directory to install the binary in. If not set, it will default to `./bin`.
    #[must_use]
    pub fn install_dir<P: AsRef<Path>>(mut self, install_dir: P) -> Self {
        self.install_dir = Some(install_dir.as_ref().to_path_buf());
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
    ///
    /// You cannot call `extract_all` if you set this.
    #[must_use]
    pub fn exe(mut self, exe: &'a str) -> Self {
        self.exe = Some(exe);
        self
    }

    /// The name to use when installing the executable. This is useful if the executable in the
    /// archive file has a name that includes a version number or platform information. If this is
    /// not set, then the executable will be installed with the name it has in the archive
    /// file. Note that this name is used as-is, so on Windows, `.exe` will not be appended to the
    /// name given.
    ///
    /// You cannot call `extract_all` if you set this.
    #[must_use]
    pub fn rename_exe_to(mut self, name: &'a str) -> Self {
        self.rename_exe_to = Some(name);
        self
    }

    /// Call this to tell `ubi` to extract all files from the archive. By default `ubi` will look
    /// for an executable in an archive file. But if this is true, it will simply unpack the archive
    /// file in the specified directory.
    ///
    /// You cannot set `exe` when this is true.
    #[must_use]
    pub fn extract_all(mut self) -> Self {
        self.extract_all = true;
        self
    }

    /// Set a token to use for API requests. If this is not set, then `ubi` will look for a token in
    /// the appropriate env var:
    ///
    /// * GitHub - `GITHUB_TOKEN`
    /// * GitLab - `CI_TOKEN`, then `GITLAB_TOKEN`.
    #[must_use]
    pub fn token(mut self, token: &'a str) -> Self {
        self.token = Some(token);
        self
    }

    /// Set a GitHub token to use for API requests. If this is not set then this will be taken from
    /// the `GITHUB_TOKEN` env var if it is set.
    #[deprecated(since = "0.6.0", note = "please use `token` instead")]
    #[must_use]
    pub fn github_token(mut self, token: &'a str) -> Self {
        self.token = Some(token);
        self
    }

    /// Set a GitLab token to use for API requests. If this is not set then this will be taken from
    /// the `CI_JOB_TOKEN` or `GITLAB_TOKEN` env var, if one of these is set. If both are set, then
    /// the value in `CI_JOB_TOKEN` will be used.
    #[deprecated(since = "0.6.0", note = "please use `token` instead")]
    #[must_use]
    pub fn gitlab_token(mut self, token: &'a str) -> Self {
        self.token = Some(token);
        self
    }

    /// Set the platform to download for. If not set it will be determined based on the current
    /// platform's OS/arch.
    #[must_use]
    pub fn platform(mut self, platform: &'a Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Set whether or not the platform uses musl as its libc. This is only relevant for Linux
    /// platforms. If this isn't set then it will be determined based on the current platform's
    /// libc. You cannot set this to `true` on a non-Linux platform.
    #[must_use]
    pub fn is_musl(mut self, is_musl: bool) -> Self {
        self.is_musl = Some(is_musl);
        self
    }

    /// Set the forge type to use for fetching assets and release information. This determines which
    /// REST API is used to get information about releases and to download the release. If this isn't
    /// set, then this will be determined from the hostname in the url, if that is set.  Otherwise,
    /// the default is GitHub.
    #[must_use]
    pub fn forge(mut self, forge: ForgeType) -> Self {
        self.forge = Some(forge);
        self
    }

    /// Set the base URL for the forge site's API. This is useful for testing or if you want to
    /// operate against an Enterprise version of GitHub or GitLab. This should be something like
    /// `https://github.my-corp.example.com/api/v4`.
    #[must_use]
    pub fn api_base_url(mut self, api_base_url: &'a str) -> Self {
        self.api_base_url = Some(api_base_url);
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
        if self.exe.is_some() && self.extract_all {
            return Err(anyhow!("You cannot set exe and enable extract_all"));
        }
        if self.rename_exe_to.is_some() && self.extract_all {
            return Err(anyhow!(
                "You cannot set rename_exe_to and enable extract_all"
            ));
        }

        let platform = self.determine_platform()?;

        self.check_musl_setting(&platform)?;

        let asset_url = self.url.map(Url::parse).transpose()?;
        let (project_name, forge_type) =
            parse_project_name(self.project, asset_url.as_ref(), self.forge.clone())?;
        let installer = self.new_installer(&project_name, &platform)?;
        let forge = self.new_forge(project_name, &forge_type)?;
        let is_musl = self.is_musl.unwrap_or_else(|| platform_is_musl(&platform));

        Ok(Ubi::new(
            forge,
            asset_url,
            AssetPicker::new(self.matching, platform, is_musl, self.extract_all),
            installer,
            reqwest_client()?,
        ))
    }

    fn new_installer(&self, project_name: &str, platform: &Platform) -> Result<Box<dyn Installer>> {
        if self.extract_all {
            let install_path = install_path(self.install_dir.as_deref(), None)?;
            Ok(Box::new(ArchiveInstaller::new(install_path)))
        } else {
            let expect_exe_stem_name = expect_exe_stem_name(self.exe, project_name);
            let install_path = install_path(
                self.install_dir.as_deref(),
                self.rename_exe_to.or(Some(expect_exe_stem_name)),
            )?;
            Ok(Box::new(ExeInstaller::new(
                install_path,
                expect_exe_stem_name.to_string(),
                platform.target_os == OS::Windows,
            )))
        }
    }

    fn new_forge(
        &self,
        project_name: String,
        forge_type: &ForgeType,
    ) -> Result<Box<dyn Forge + Send + Sync>> {
        forge_type.make_forge_impl(
            project_name,
            self.tag.map(String::from),
            self.api_base_url.map(String::from),
            self.token.map(String::from),
        )
    }

    fn determine_platform(&self) -> Result<Platform> {
        if let Some(p) = self.platform {
            Ok(p.clone())
        } else {
            let req = PlatformReq::from_str(Self::TARGET)?;
            Platform::ALL
                .iter()
                .find(|p| req.matches(p))
                .cloned()
                .ok_or(anyhow!(
                    "Could not find any platform matching {}",
                    Self::TARGET
                ))
        }
    }

    fn check_musl_setting(&self, platform: &Platform) -> Result<()> {
        if self.is_musl.unwrap_or_default() && platform.target_os != OS::Linux {
            return Err(anyhow!(
                "You cannot set is_musl to true on a non-Linux platform - the current platform is {}",
                platform.target_os,
            ));
        }
        Ok(())
    }
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

fn install_path(install_dir: Option<&Path>, exe: Option<&str>) -> Result<PathBuf> {
    let mut install_dir = if let Some(install_dir) = install_dir {
        install_dir.to_path_buf()
    } else {
        let mut install_dir = env::current_dir()?;
        install_dir.push("bin");
        install_dir
    };
    if let Some(exe) = exe {
        install_dir.push(exe);
    }
    debug!("install path = {}", install_dir.to_string_lossy());
    Ok(install_dir)
}

fn expect_exe_stem_name<'a>(exe: Option<&'a str>, project_name: &'a str) -> &'a str {
    let name = if let Some(exe) = exe {
        exe
    } else {
        // We know that this contains a slash because it already went through `parse_project_name`
        // successfully.
        project_name.split('/').next_back().unwrap()
    };

    debug!("exe name = {name}");
    name
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

fn reqwest_client() -> Result<Client> {
    let builder = Client::builder().gzip(true);

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&format!("ubi version {}", super::VERSION))?,
    );
    Ok(builder.default_headers(headers).build()?)
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

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
            let (project_name, forge_type) = super::parse_project_name(Some(p), None, None)?;
            assert_eq!(
                project_name, org_and_repo,
                "got the right project from --project {p}",
            );
            assert_eq!(forge_type, ForgeType::GitHub);

            let (project_name, forge_type) =
                super::parse_project_name(Some(p), None, Some(ForgeType::GitHub))?;
            assert_eq!(
                project_name, org_and_repo,
                "got the right project from --project {p}",
            );
            assert_eq!(forge_type, ForgeType::GitHub);
        }

        {
            let url = Url::parse("https://github.com/houseabsolute/precious/releases/download/v0.1.7/precious-Linux-x86_64-musl.tar.gz")?;
            let (project_name, forge_type) = super::parse_project_name(None, Some(&url), None)?;
            assert_eq!(
                project_name, "houseabsolute/precious",
                "got the right project from the --url",
            );
            assert_eq!(forge_type, ForgeType::GitHub);

            let (project_name, forge_type) =
                super::parse_project_name(None, Some(&url), Some(ForgeType::GitHub))?;
            assert_eq!(
                project_name, "houseabsolute/precious",
                "got the right project from the --url",
            );
            assert_eq!(forge_type, ForgeType::GitHub);
        }

        Ok(())
    }

    #[test_case(
        None,
        "houseabsolute/precious",
        "precious";
        "no exe or exe_name"
    )]
    #[test_case(
        Some("foo"),
        "houseabsolute/precious",
        "foo";
        "passed exe"
    )]
    fn expect_exe_stem_name(
        exe: Option<&'static str>,
        project_name: &'static str,
        expect: &'static str,
    ) {
        assert_eq!(super::expect_exe_stem_name(exe, project_name), expect);
    }
}
