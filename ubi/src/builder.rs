/// The `builder` module contains the `UbiBuilder` struct which is used to create a `Ubi` instance.
use crate::{forge::ForgeType, Ubi};
use anyhow::{anyhow, Result};
use platforms::{Platform, PlatformReq, OS};
use std::{env, path::PathBuf, str::FromStr};
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
    github_token: Option<&'a str>,
    gitlab_token: Option<&'a str>,
    platform: Option<&'a Platform>,
    is_musl: Option<bool>,
    url_base: Option<String>,
    forge: Option<ForgeType>,
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

    /// Set a GitLab token to use for API requests. If this is not set then this will be taken from
    ////the `CI_JOB_TOKEN` or `GITLAB_TOKEN` env var if one of these is set. If both are set, then
    /// the value `CI_JOB_TOKEN` will be used.
    #[must_use]
    pub fn gitlab_token(mut self, token: &'a str) -> Self {
        self.gitlab_token = Some(token);
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

    /// Set the base URL for the forge site's API. This is useful for testing or if you want to operate
    /// against an Enterprise version of GitHub or GitLab,
    #[must_use]
    pub fn url_base(mut self, url_base: String) -> Self {
        self.url_base = Some(url_base);
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

        if self.is_musl.unwrap_or_default() && platform.target_os != OS::Linux {
            return Err(anyhow!(
                "You cannot set is_musl to true on a non-Linux platform - the current platform is {}",
                platform.target_os,
            ));
        }

        Ubi::new(
            self.project,
            self.tag,
            self.url,
            self.install_dir,
            self.matching,
            self.exe,
            self.github_token,
            self.gitlab_token,
            platform,
            match self.is_musl {
                Some(m) => m,
                None => platform_is_musl(platform),
            },
            self.url_base.as_deref(),
            self.forge,
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
