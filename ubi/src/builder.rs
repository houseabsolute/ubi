/// The `builder` module contains the `UbiBuilder` struct which is used to create a `Ubi` instance.
use crate::{
    forge::{redacted, ForgeType},
    installer::{ArchiveInstaller, ExeInstaller, Installer},
    picker::AssetPicker,
    ubi::Ubi,
};
use anyhow::{anyhow, Context, Result};
use log::debug;
use platforms::{Os, Platform, PlatformReq};
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
#[derive(Default)]
#[allow(clippy::module_name_repetitions)]
pub struct UbiBuilder<'a> {
    project: Option<&'a str>,
    tag: Option<&'a str>,
    url: Option<&'a str>,
    install_dir: Option<PathBuf>,
    matching: Option<&'a str>,
    matching_regex: Option<&'a str>,
    exe: Option<&'a str>,
    rename_exe_to: Option<&'a str>,
    extract_all: bool,
    token: Option<&'a str>,
    platform: Option<&'a Platform>,
    is_musl: Option<bool>,
    api_base_url: Option<&'a str>,
    forge: Option<ForgeType>,
    min_age_days: Option<u32>,
}

// Debug output ends up in logs and panic messages, so we hide the token.
impl std::fmt::Debug for UbiBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UbiBuilder")
            .field("project", &self.project)
            .field("tag", &self.tag)
            .field("url", &self.url)
            .field("install_dir", &self.install_dir)
            .field("matching", &self.matching)
            .field("matching_regex", &self.matching_regex)
            .field("exe", &self.exe)
            .field("rename_exe_to", &self.rename_exe_to)
            .field("extract_all", &self.extract_all)
            .field("token", &redacted(self.token))
            .field("platform", &self.platform)
            .field("is_musl", &self.is_musl)
            .field("api_base_url", &self.api_base_url)
            .field("forge", &self.forge)
            .field("min_age_days", &self.min_age_days)
            .finish()
    }
}

impl<'a> UbiBuilder<'a> {
    /// Returns a new empty `UbiBuilder`.
    #[must_use]
    pub fn new() -> Self {
        UbiBuilder::default()
    }

    /// Set the project to download from. This can either be just the org/name, like
    /// `houseabsolute/precious`, or the complete forge site URL to the project, like
    /// `https://github.com/houseabsolute/precious`, `https://gitlab.com/gitlab-org/cli`, or
    /// `https://codeberg.org/codeberg/cli`. It also accepts a URL to any page in the project, like
    /// `https://github.com/houseabsolute/precious/releases`.
    ///
    /// You must set this or set `url`, but not both.
    #[must_use]
    pub fn project(mut self, project: &'a str) -> Self {
        self.project = Some(project);
        self
    }

    /// Set the tag to download. By default the most recent release is downloaded.
    ///
    /// You cannot set this with the `url` or `min_age_days` options.
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
    /// The token is only sent if this URL is on the forge's API or project site. For a private
    /// repo on GitHub Enterprise or a self-hosted forge, also set `api_base_url` for that server.
    ///
    /// By default the executable name is taken from the second path segment of the URL, like
    /// `repo` in `https://github.com/owner/repo/releases/download/v1.0.0/repo.tar.gz`. If the URL
    /// has fewer than two path segments, you must also set `exe` or `extract_all`. This can also be
    /// a GitHub API asset URL, like `https://api.github.com/repos/owner/repo/releases/assets/12345`.
    /// For GitHub Enterprise, also set `api_base_url` so the project name can be found in it.
    ///
    /// You must set this or set `project`, but not both. You cannot set this with the `tag` or
    /// `min_age_days` options.
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

    /// Set a regular expression string that will be matched against release filenames before
    /// matching against your OS/arch. If the pattern yields a single match, that release will be
    /// selected. If no matches are found, then the `Ubi::install_binary` method will return an
    /// error when it is run.
    #[must_use]
    pub fn matching_regex(mut self, matching_regex: &'a str) -> Self {
        self.matching_regex = Some(matching_regex);
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

    /// Set the minimum age in days for releases. Only releases at least this many days old will be
    /// installed. This is useful for mitigating supply chain attacks. It's especially useful for
    /// projects that use GitHub's immutable releases feature.
    ///
    /// You cannot set this with the `tag` or `url` options.
    #[must_use]
    pub fn min_age_days(mut self, days: u32) -> Self {
        self.min_age_days = Some(days);
        self
    }

    /// Set a token to use for API requests. If this is not set, then `ubi` will look for a token in
    /// the appropriate env var:
    ///
    /// * GitHub - `GITHUB_TOKEN`
    /// * GitLab - `CI_JOB_TOKEN`, then `GITLAB_TOKEN`.
    /// * Codeberg/Forgejo - `CODEBERG_TOKEN`, then `FORGEJO_TOKEN`.
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
        if let Some(days) = self.min_age_days {
            if self.url.is_some() {
                return Err(anyhow!("You cannot set min_age_days with url"));
            }
            if self.tag.is_some() {
                return Err(anyhow!("You cannot set min_age_days with tag"));
            }
            if days == 0 {
                return Err(anyhow!(
                    "min_age_days must be a positive number (greater than 0)"
                ));
            }
        }

        let platform = self.determine_platform()?;

        self.check_musl_setting(&platform)?;

        let asset_url = match self.url {
            Some(url) => {
                Some(Url::parse(url).with_context(|| format!("failed to parse URL: {url}"))?)
            }
            None => None,
        };
        let (project_name, forge_type) = match parse_project_name(
            self.project,
            asset_url.as_ref(),
            self.forge.clone(),
            self.api_base_url,
        ) {
            Ok(r) => r,
            // With a URL, the project name is only used to guess the exe name, since we never
            // call the forge API. So a URL with no project in its path is fine as long as we
            // don't need that guess.
            Err(e) => match &asset_url {
                Some(u) if self.exe.is_some() || self.extract_all => (
                    String::new(),
                    self.forge.clone().unwrap_or_else(|| ForgeType::from_url(u)),
                ),
                Some(_) => {
                    return Err(e.context(
                        "could not guess the executable name from the URL, so set exe (--exe) \
                             to the name of the executable, or set extract_all (--extract-all)",
                    ))
                }
                None => return Err(e),
            },
        };
        let installer = self.new_installer(&project_name, &platform)?;
        let forge = forge_type.new_forge(
            project_name,
            self.tag.map(String::from),
            self.api_base_url.map(String::from),
            self.token.map(String::from),
        )?;
        let is_musl = self.is_musl.unwrap_or_else(|| platform_is_musl(&platform));

        Ok(Ubi::new(
            forge,
            asset_url,
            AssetPicker::new(
                self.matching,
                self.matching_regex,
                platform,
                is_musl,
                self.extract_all,
            ),
            installer,
            reqwest_client()?,
            self.min_age_days,
        ))
    }

    fn new_installer(&self, project_name: &str, platform: &Platform) -> Result<Box<dyn Installer>> {
        if self.extract_all {
            let install_path =
                install_path(self.install_dir.as_deref(), None).with_context(|| {
                    format!("failed to determine install path for project {project_name}")
                })?;
            Ok(Box::new(ArchiveInstaller::new(install_path)))
        } else {
            let expect_exe_stem_name = expect_exe_stem_name(self.exe, project_name);
            let install_path = install_path(
                self.install_dir.as_deref(),
                self.rename_exe_to.or(Some(expect_exe_stem_name)),
            )
            .with_context(|| {
                format!("failed to determine install path for executable {expect_exe_stem_name}")
            })?;
            Ok(Box::new(ExeInstaller::new(
                install_path,
                self.rename_exe_to.is_some(),
                self.exe.is_some(),
                expect_exe_stem_name.to_string(),
                platform.target_os == Os::Windows,
            )))
        }
    }

    fn determine_platform(&self) -> Result<Platform> {
        if let Some(p) = self.platform {
            Ok(p.clone())
        } else {
            let req = PlatformReq::from_str(Self::TARGET)
                .with_context(|| format!("failed to parse platform target: {}", Self::TARGET))?;
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
        if self.is_musl.unwrap_or_default() && platform.target_os != Os::Linux {
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
    api_base: Option<&str>,
) -> Result<(String, ForgeType)> {
    let (parsed, from) = if let Some(project) = project {
        // We check for the whole scheme because an owner name like `httpie` also starts with
        // "http".
        let lower = project.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            (
                Url::parse(project)
                    .with_context(|| format!("failed to parse project URL: {project}"))?,
                format!("--project {project}"),
            )
        } else {
            let base = forge.clone().unwrap_or_default().project_base_url();
            (
                base.join(project).with_context(|| {
                    format!("failed to construct project URL from '{project}' with base URL {base}")
                })?,
                format!("--project {project}"),
            )
        }
    } else if let Some(u) = url {
        (u.clone(), format!("--url {u}"))
    } else {
        unreachable!(
            "Did not get a --project or --url argument but that should be checked in main.rs"
        );
    };

    // We can only guess the forge from the host for the big public forges, so a forge the caller
    // gave us must win. Otherwise a self-hosted GitLab or Forgejo would be treated as GitHub.
    let forge_type = forge.unwrap_or_else(|| ForgeType::from_url(&parsed));
    if project.is_none() {
        if let Some(project_name) = project_name_from_api_url(&parsed, &forge_type, api_base) {
            return Ok((project_name, forge_type));
        }
    }
    let project_name = forge_type
        .parse_project_name_from_url(&parsed, &from)
        .with_context(|| format!("failed to parse project name from URL {parsed}"))?;

    Ok((project_name, forge_type))
}

// A `--url` can be a release asset URL from the forge's API, like
// `https://api.github.com/repos/owner/repo/releases/assets/12345`. The path of a project URL starts
// with the owner and repo, but this one has the API base path and `repos` first.
fn project_name_from_api_url(
    url: &Url,
    forge_type: &ForgeType,
    api_base: Option<&str>,
) -> Option<String> {
    // Only GitHub serves the asset file itself from an API asset URL. Forgejo returns JSON
    // metadata there, and GitLab API paths use a project ID or an encoded path instead.
    if *forge_type != ForgeType::GitHub {
        return None;
    }
    // A bad API base URL is reported when we make the forge, so here we just treat it as no match.
    let base = match api_base {
        Some(b) => Url::parse(b).ok()?,
        None => forge_type.api_base_url(),
    };
    if url.origin() != base.origin() {
        return None;
    }

    let base_parts = base.path().split('/').filter(|s| !s.is_empty());
    let mut parts = url.path().split('/').filter(|s| !s.is_empty());
    for b in base_parts {
        if parts.next() != Some(b) {
            return None;
        }
    }
    if parts.next() != Some("repos") {
        return None;
    }
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
        return None;
    };
    debug!("Parsed API URL {url} = {owner} / {repo}");
    Some(format!("{owner}/{repo}"))
}

fn install_path(install_dir: Option<&Path>, exe: Option<&str>) -> Result<PathBuf> {
    let mut install_dir = if let Some(install_dir) = install_dir {
        install_dir.to_path_buf()
    } else {
        let mut install_dir = env::current_dir().context("failed to get current directory")?;
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
        // successfully. The project name is only empty for a URL with no project in it, and then
        // `exe` must be set.
        project_name.split('/').next_back().unwrap()
    };

    debug!("exe name = {name}");
    name
}

fn platform_is_musl(platform: &Platform) -> bool {
    if platform.target_os != Os::Linux {
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

// We enable `reqwest`'s `rustls-no-provider` feature rather than its `rustls` feature, because the
// latter pulls in `aws-lc-rs`, which needs a C/C++ toolchain and does not build under `cross` for
// all of the platforms we support. That means no crypto provider is compiled in by default, and
// `reqwest` will panic when it builds a client unless one has been installed, so we install `ring`
// here.
//
// Installing the default provider is a process-wide operation that can only succeed once. If it
// fails, that's because something else - most likely the embedding application - already installed
// one, which is fine, so we ignore the error.
#[cfg(feature = "rustls-tls")]
pub(crate) fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(not(feature = "rustls-tls"))]
#[allow(clippy::missing_const_for_fn)]
pub(crate) fn install_crypto_provider() {}

fn reqwest_client() -> Result<Client> {
    install_crypto_provider();

    let builder = Client::builder().gzip(true);

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&format!("ubi version {}", super::VERSION))
            .context("failed to create User-Agent header value")?,
    );
    builder
        .default_headers(headers)
        .build()
        .context("failed to build HTTP client")
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    // Debug output ends up in logs and panic messages, so it must never show the token.
    #[test]
    fn builder_debug_hides_the_token() {
        let builder = UbiBuilder::new()
            .project("owner/repo")
            .token("secret-token");
        let debug = format!("{builder:?}");
        assert!(!debug.contains("secret-token"));
        assert!(debug.contains(r#"token: Some("<redacted>")"#));
        assert!(debug.contains("owner/repo"));
    }

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
            let (project_name, forge_type) = super::parse_project_name(Some(p), None, None, None)?;
            assert_eq!(
                project_name, org_and_repo,
                "got the right project from --project {p}",
            );
            assert_eq!(forge_type, ForgeType::GitHub);

            let (project_name, forge_type) =
                super::parse_project_name(Some(p), None, Some(ForgeType::GitHub), None)?;
            assert_eq!(
                project_name, org_and_repo,
                "got the right project from --project {p}",
            );
            assert_eq!(forge_type, ForgeType::GitHub);
        }

        {
            let url = Url::parse("https://github.com/houseabsolute/precious/releases/download/v0.1.7/precious-Linux-x86_64-musl.tar.gz")?;
            let (project_name, forge_type) =
                super::parse_project_name(None, Some(&url), None, None)?;
            assert_eq!(
                project_name, "houseabsolute/precious",
                "got the right project from the --url",
            );
            assert_eq!(forge_type, ForgeType::GitHub);

            let (project_name, forge_type) =
                super::parse_project_name(None, Some(&url), Some(ForgeType::GitHub), None)?;
            assert_eq!(
                project_name, "houseabsolute/precious",
                "got the right project from the --url",
            );
            assert_eq!(forge_type, ForgeType::GitHub);
        }

        Ok(())
    }

    // A `--url` can point at a release asset through the forge's API, and its path has an API
    // prefix and a `repos` segment before the owner and repo.
    #[rstest]
    #[case::github(
        "https://api.github.com/repos/owner/repo/releases/assets/12345",
        None,
        "owner/repo",
        ForgeType::GitHub
    )]
    #[case::github_enterprise(
        "https://ghe.example.com/api/v3/repos/owner/repo/releases/assets/12345",
        Some("https://ghe.example.com/api/v3"),
        "owner/repo",
        ForgeType::GitHub
    )]
    // Without the API base URL, this looks like a normal project URL for the `api/v3` project.
    #[case::github_enterprise_without_api_base(
        "https://ghe.example.com/api/v3/repos/owner/repo/releases/assets/12345",
        None,
        "api/v3",
        ForgeType::GitHub
    )]
    fn parse_project_name_from_an_api_url(
        #[case] url: &str,
        #[case] api_base: Option<&str>,
        #[case] expect: &str,
        #[case] expect_forge: ForgeType,
    ) -> Result<()> {
        let url = Url::parse(url)?;
        let (project_name, forge_type) =
            super::parse_project_name(None, Some(&url), None, api_base)?;
        assert_eq!(project_name, expect);
        assert_eq!(forge_type, expect_forge);

        Ok(())
    }

    // Only a value with a URL scheme is a URL. An owner name can start with "http" too.
    #[rstest]
    #[case::owner_starts_with_http("httpie/cli", "httpie/cli")]
    #[case::owner_is_https("https/repo", "https/repo")]
    #[case::upper_case_scheme("HTTPS://github.com/httpie/cli", "httpie/cli")]
    fn parse_project_name_only_treats_a_scheme_as_a_url(
        #[case] project: &str,
        #[case] expect: &str,
    ) -> Result<()> {
        let (project_name, forge_type) =
            super::parse_project_name(Some(project), None, None, None)?;
        assert_eq!(project_name, expect);
        assert_eq!(forge_type, ForgeType::GitHub);

        Ok(())
    }

    // An explicit forge must win over the guess we make from the URL's host, since a self-hosted
    // forge can be on any host.
    #[rstest]
    #[case::gitlab_project_url(
        Some("https://gitlab.example.com/owner/repo"),
        None,
        ForgeType::GitLab
    )]
    #[case::forgejo_project_url(
        Some("https://git.example.com/owner/repo"),
        None,
        ForgeType::Forgejo
    )]
    #[case::gitlab_asset_url(
        None,
        Some("https://gitlab.example.com/owner/repo/-/releases/v1.0.0/downloads/tool.tar.gz"),
        ForgeType::GitLab
    )]
    #[case::forgejo_asset_url(
        None,
        Some("https://git.example.com/owner/repo/releases/download/v1.0.0/tool.tar.gz"),
        ForgeType::Forgejo
    )]
    #[case::short_name(Some("owner/repo"), None, ForgeType::GitLab)]
    fn parse_project_name_uses_the_given_forge(
        #[case] project: Option<&str>,
        #[case] url: Option<&str>,
        #[case] forge: ForgeType,
    ) -> Result<()> {
        let url = url.map(Url::parse).transpose()?;
        let (project_name, forge_type) =
            super::parse_project_name(project, url.as_ref(), Some(forge.clone()), None)?;
        assert_eq!(project_name, "owner/repo");
        assert_eq!(forge_type, forge);

        Ok(())
    }

    #[rstest]
    #[case::precious(None, "houseabsolute/precious", "precious")]
    #[case::foo(Some("foo"), "houseabsolute/precious", "foo")]
    #[case::terra_transformer(
        None,
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer",
        "terra-transformer"
    )]
    fn expect_exe_stem_name(
        #[case] exe: Option<&'static str>,
        #[case] project_name: &'static str,
        #[case] expect: &'static str,
    ) {
        assert_eq!(super::expect_exe_stem_name(exe, project_name), expect);
    }

    // With `--exe` or `--extract-all`, ubi doesn't need a project name to know what to install, so
    // a download URL with only one path segment should work.
    #[rstest]
    #[case::exe(Some("tool"), false)]
    #[case::extract_all(None, true)]
    fn url_with_one_path_segment_works_with_exe_or_extract_all(
        #[case] exe: Option<&str>,
        #[case] extract_all: bool,
    ) {
        let mut builder = UbiBuilder::new().url("https://example.com/tool-linux-x86_64.tar.gz");
        if let Some(exe) = exe {
            builder = builder.exe(exe);
        }
        if extract_all {
            builder = builder.extract_all();
        }
        if let Err(e) = builder.build() {
            panic!("expected build to succeed, got error: {e:#}");
        }
    }

    #[test]
    fn url_with_one_path_segment_and_no_exe_suggests_exe() {
        let res = UbiBuilder::new()
            .url("https://example.com/tool-linux-x86_64.tar.gz")
            .build();
        match res {
            Ok(_) => panic!("expected build to fail"),
            Err(e) => assert!(format!("{e:#}").contains("--exe"), "got error: {e:#}"),
        }
    }

    #[test]
    fn min_age_days_zero_validation() {
        let result = UbiBuilder::new()
            .project("houseabsolute/ubi")
            .min_age_days(0)
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("min_age_days must be a positive number"));
    }
}
