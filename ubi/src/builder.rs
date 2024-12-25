/// The `builder` module contains the `UbiBuilder` struct which is used to create a `Ubi` instance.
use crate::{
    forge::{Forge, ForgeType},
    github::GitHub,
    gitlab::GitLab,
    installer::Installer,
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
use std::{env, fs::create_dir_all, path::PathBuf, str::FromStr};
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
    /// the `CI_JOB_TOKEN` or `GITLAB_TOKEN` env var, if one of these is set. If both are set, then
    /// the value in `CI_JOB_TOKEN` will be used.
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
    /// against an Enterprise version of GitHub or GitLab.
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

        let platform = self.determine_platform()?;

        self.check_musl_setting(&platform)?;

        let asset_url = self.url.map(Url::parse).transpose()?;
        let (project_name, forge_type) =
            parse_project_name(self.project, asset_url.as_ref(), self.forge.clone())?;
        let exe = exe_name(self.exe, &project_name, &platform);
        let forge = self.new_forge(project_name, &forge_type)?;
        let install_path = install_path(self.install_dir, &exe)?;
        let is_musl = self.is_musl.unwrap_or_else(|| platform_is_musl(&platform));

        Ok(Ubi::new(
            forge,
            asset_url,
            AssetPicker::new(self.matching, platform, is_musl),
            Installer::new(install_path, exe),
            reqwest_client()?,
        ))
    }

    fn new_forge(&self, project_name: String, forge_type: &ForgeType) -> Result<Box<dyn Forge>> {
        let api_base = self.url_base.as_deref().map(Url::parse).transpose()?;
        Ok(match forge_type {
            ForgeType::GitHub => Box::new(GitHub::new(
                project_name,
                self.tag.map(String::from),
                api_base,
                self.github_token,
            )),
            ForgeType::GitLab => Box::new(GitLab::new(
                project_name,
                self.tag.map(String::from),
                api_base,
                self.gitlab_token,
            )),
        })
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
            assert_eq!(super::exe_name(t.exe, t.project_name, platform), t.expect);
        }

        Ok(())
    }
}
