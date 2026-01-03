use crate::{forgejo, github, gitlab, ubi::Asset};
use anyhow::{anyhow, Context, Result};
use log::debug;
use reqwest::{
    header::{HeaderValue, ACCEPT, AUTHORIZATION},
    Client, RequestBuilder, Response,
};
use serde::Deserialize;
use std::env;
use url::Url;

// It'd be nice to use clap::ValueEnum here, but then we'd need to add clap as a dependency for the
// library code, which would be annoying for downstream users who just want to use the library.
#[derive(
    strum::AsRefStr, Clone, Debug, Default, strum::EnumString, PartialEq, Eq, strum::VariantNames,
)]
#[allow(clippy::module_name_repetitions)]
pub enum ForgeType {
    #[strum(serialize = "forgejo")]
    Forgejo,
    #[strum(serialize = "github")]
    #[default]
    GitHub,
    #[strum(serialize = "gitlab")]
    GitLab,
}

#[derive(Debug)]
pub(crate) struct Forge {
    project_name: String,
    tag: Option<String>,
    api_base_url: Url,
    token: Option<String>,
    #[allow(clippy::struct_field_names)] // We can't call this `type`.
    forge_type: ForgeType,
}

unsafe impl Send for Forge {}
unsafe impl Sync for Forge {}

// Unified release structure that works for both GitHub and GitLab
// using serde attributes to handle the differences
#[derive(Debug, Deserialize)]
struct Release {
    // Accept both "published_at" (GitHub) and "released_at" (GitLab)
    #[serde(alias = "published_at", alias = "released_at")]
    date: chrono::DateTime<chrono::Utc>,

    // Handle both direct array (GitHub) and nested object (GitLab)
    #[serde(deserialize_with = "deserialize_assets")]
    assets: Vec<Asset>,
}

// Custom deserializer to handle both GitHub's direct array and GitLab's nested structure
fn deserialize_assets<'de, D>(deserializer: D) -> Result<Vec<Asset>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum AssetsFormat {
        Direct(Vec<Asset>),
        Nested { links: Vec<Asset> },
    }

    match AssetsFormat::deserialize(deserializer)? {
        AssetsFormat::Direct(assets) => Ok(assets),
        AssetsFormat::Nested { links } => Ok(links),
    }
}

impl Forge {
    pub(crate) async fn fetch_assets(&self, client: &Client) -> Result<Vec<Asset>> {
        debug!("Fetching assets for project `{}`", self.project_name);
        let response = self.make_release_info_request(client).await?;
        Ok(response
            .json::<Release>()
            .await
            .context("failed to parse release JSON response")?
            .assets)
    }

    pub(crate) async fn fetch_assets_with_min_age(
        &self,
        client: &Client,
        min_age_days: u32,
    ) -> Result<Vec<Asset>> {
        debug!(
            "Fetching assets for project `{}` with minimum age of {min_age_days} days",
            self.project_name
        );

        let min_date = chrono::Utc::now() - chrono::Duration::days(i64::from(min_age_days));
        for release in self.fetch_releases_list(client).await? {
            if release.date <= min_date {
                debug!(
                    "Found release from {} (older than {min_age_days} days)",
                    release.date
                );
                return Ok(release.assets);
            }
        }

        Err(anyhow!("No releases found older than {min_age_days} days"))
    }

    async fn fetch_releases_list(&self, client: &Client) -> Result<Vec<Release>> {
        let url = self
            .forge_type
            .releases_list_url(&self.project_name, self.api_base_url.clone());
        let resp = self
            .make_api_request(client, url, "Getting releases list")
            .await?;

        resp.json::<Vec<Release>>()
            .await
            .context("failed to parse releases list JSON response")
    }

    async fn make_release_info_request(&self, client: &Client) -> Result<Response> {
        let url = self.forge_type.release_info_url(
            &self.project_name,
            self.api_base_url.clone(),
            self.tag.as_deref(),
        );
        self.make_api_request(client, url, "Getting release info")
            .await
    }

    async fn make_api_request(
        &self,
        client: &Client,
        url: Url,
        log_message: &str,
    ) -> Result<Response> {
        debug!("{log_message} from `{url}`");

        let mut req_builder = client.get(url.clone()).header(
            ACCEPT,
            HeaderValue::from_str("application/json")
                .context("failed to create header value for Accept header")?,
        );
        req_builder = self.maybe_add_token_header(req_builder)?;
        let req = req_builder
            .build()
            .with_context(|| format!("failed to build HTTP request for {url}"))?;
        let resp = client
            .execute(req)
            .await
            .with_context(|| format!("failed to execute HTTP request to {url}"))?;

        resp.error_for_status()
            .with_context(|| format!("HTTP request to {url} returned an error status"))
    }

    pub(crate) fn maybe_add_token_header(
        &self,
        mut req_builder: RequestBuilder,
    ) -> Result<RequestBuilder> {
        if let Some(token) = self.token.as_deref() {
            debug!("Adding token to {} request.", self.forge_type.forge_name());
            let bearer = format!("Bearer {token}");
            let mut auth_val = HeaderValue::from_str(&bearer)
                .context("failed to create header value for Authorization header")?;
            auth_val.set_sensitive(true);
            req_builder = req_builder.header(AUTHORIZATION, auth_val);
        } else {
            debug!("No token given.");
        }
        Ok(req_builder)
    }
}

impl ForgeType {
    pub(crate) fn from_url(url: &Url) -> ForgeType {
        let Some(domain) = url.domain() else {
            return ForgeType::default();
        };

        if Some(domain) == forgejo::PROJECT_BASE_URL.domain() {
            ForgeType::Forgejo
        } else if Some(domain) == gitlab::PROJECT_BASE_URL.domain() {
            ForgeType::GitLab
        } else {
            ForgeType::default()
        }
    }

    pub(crate) fn parse_project_name_from_url(&self, url: &Url, from: &str) -> Result<String> {
        match self {
            ForgeType::Forgejo | ForgeType::GitHub => {
                github::parse_project_name_from_url(url, from)
            }
            ForgeType::GitLab => gitlab::parse_project_name_from_url(url, from),
        }
    }

    pub(crate) fn project_base_url(&self) -> Url {
        match self {
            ForgeType::Forgejo => forgejo::PROJECT_BASE_URL.clone(),
            ForgeType::GitHub => github::PROJECT_BASE_URL.clone(),
            ForgeType::GitLab => gitlab::PROJECT_BASE_URL.clone(),
        }
    }

    pub(crate) fn api_base_url(&self) -> Url {
        match self {
            ForgeType::Forgejo => forgejo::DEFAULT_API_BASE_URL.clone(),
            ForgeType::GitHub => github::DEFAULT_API_BASE_URL.clone(),
            ForgeType::GitLab => gitlab::DEFAULT_API_BASE_URL.clone(),
        }
    }

    pub(crate) fn env_var_names(&self) -> &'static [&'static str] {
        match self {
            ForgeType::Forgejo => &["CODEBERG_TOKEN", "FORGEJO_TOKEN"],
            ForgeType::GitHub => &["GITHUB_TOKEN"],
            ForgeType::GitLab => &["CI_TOKEN", "GITLAB_TOKEN"],
        }
    }

    pub(crate) fn forge_name(&self) -> &'static str {
        match self {
            ForgeType::Forgejo => "Forgjo",
            ForgeType::GitHub => "GitHub",
            ForgeType::GitLab => "GitLab",
        }
    }

    pub(crate) fn new_forge(
        self,
        project_name: String,
        tag: Option<String>,
        api_base: Option<String>,
        mut token: Option<String>,
    ) -> Result<Forge> {
        let api_base_url = if let Some(api_base) = api_base {
            Url::parse(&api_base)
                .with_context(|| format!("failed to parse API base URL: {api_base}"))?
        } else {
            self.api_base_url()
        };

        if token.is_none() {
            for name in self.env_var_names() {
                token = env::var(name).ok();
                if token.is_some() {
                    debug!(
                        "Using {} token from the {name} environment variable.",
                        self.forge_name()
                    );
                    break;
                }
            }
        }

        Ok(Forge {
            project_name,
            tag,
            api_base_url,
            token,
            forge_type: self,
        })
    }

    fn release_info_url(&self, project_name: &str, url: Url, tag: Option<&str>) -> Url {
        match self {
            ForgeType::Forgejo | ForgeType::GitHub => {
                github::release_info_url(project_name, url, tag)
            }
            ForgeType::GitLab => gitlab::release_info_url(project_name, url, tag),
        }
    }

    fn releases_list_url(&self, project_name: &str, mut url: Url) -> Url {
        match self {
            ForgeType::Forgejo | ForgeType::GitHub => {
                // /repos/{owner}/{repo}/releases
                let mut parts = project_name.split('/');
                let owner = parts.next().unwrap();
                let repo = parts.next().unwrap();
                url.path_segments_mut()
                    .expect("could not get path segments for url")
                    .push("repos")
                    .push(owner)
                    .push(repo)
                    .push("releases");
                url
            }
            ForgeType::GitLab => {
                // /projects/{project}/releases
                url.path_segments_mut()
                    .expect("could not get path segments for url")
                    .push("projects")
                    .push(project_name)
                    .push("releases");
                url
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use reqwest::Client;
    use serial_test::serial;
    use std::env;
    use test_log::test;

    #[test(tokio::test)]
    #[serial]
    async fn forgejo_fetch_assets_without_token() -> Result<()> {
        forgejo_fetch_assets(None, None).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn forgejo_fetch_assets_with_token() -> Result<()> {
        forgejo_fetch_assets(None, Some("1234")).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn forgejo_fetch_assets_with_tag() -> Result<()> {
        forgejo_fetch_assets(Some("v1.0.0"), None).await
    }

    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    struct ForgejoRelease {
        assets: Vec<ForgejoAsset>,
        published_at: chrono::DateTime<chrono::Utc>,
    }
    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    struct ForgejoAsset {
        name: String,
        browser_download_url: Url,
    }

    async fn forgejo_fetch_assets(tag: Option<&str>, token: Option<&str>) -> Result<()> {
        let vars = env::vars();
        env::remove_var("CODEBERG_TOKEN");
        env::remove_var("FORGEJO_TOKEN");

        let asset_url = Url::parse("https://codeberg.org/repos/some/project/releases/assets/1")?;
        let assets = vec![ForgejoAsset {
            name: "asset1".to_string(),
            browser_download_url: asset_url.clone(),
        }];

        let expect_path = if let Some(tag) = tag {
            format!("/repos/some/project/releases/tags/{tag}")
        } else {
            "/repos/some/project/releases/latest".to_string()
        };
        let authorization_header_matcher = {
            if let Some(token) = token {
                mockito::Matcher::Exact(format!("Bearer {token}"))
            } else {
                mockito::Matcher::Missing
            }
        };
        let mut server = Server::new_async().await;

        let m = server
            .mock("GET", expect_path.as_str())
            .match_header("Authorization", authorization_header_matcher)
            .with_status(200)
            .with_body(serde_json::to_string(&ForgejoRelease {
                assets,
                published_at: chrono::Utc::now(),
            })?)
            .create_async()
            .await;

        let forge = ForgeType::Forgejo.new_forge(
            "some/project".to_string(),
            tag.map(String::from),
            Some(server.url()),
            token.map(String::from),
        )?;

        let client = Client::new();
        let got_assets = forge.fetch_assets(&client).await?;
        let expect_assets = vec![Asset {
            name: "asset1".to_string(),
            url: asset_url,
        }];
        assert_eq!(got_assets, expect_assets);

        m.assert_async().await;

        for (k, v) in vars {
            env::set_var(k, v);
        }

        Ok(())
    }

    #[test]
    fn forgejo_api_base_url() -> Result<()> {
        let url = ForgeType::Forgejo.release_info_url(
            "houseabsolute/ubi",
            Url::parse("https://codeberg.org/api/v1")?,
            None,
        );
        assert_eq!(
            url.as_str(),
            "https://codeberg.org/api/v1/repos/houseabsolute/ubi/releases/latest"
        );
        Ok(())
    }

    #[test(tokio::test)]
    #[serial]
    async fn github_fetch_assets_without_token() -> Result<()> {
        github_fetch_assets(None, None).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn github_fetch_assets_with_token() -> Result<()> {
        github_fetch_assets(None, Some("ghp_fakeToken")).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn github_fetch_assets_with_tag() -> Result<()> {
        github_fetch_assets(Some("v1.0.0"), None).await
    }

    async fn github_fetch_assets(tag: Option<&str>, token: Option<&str>) -> Result<()> {
        let vars = env::vars();
        env::remove_var("GITHUB_TOKEN");

        let assets = vec![Asset {
            name: "asset1".to_string(),
            url: Url::parse("https://api.github.com/repos/houseabsolute/ubi/releases/assets/1")?,
        }];

        let expect_path = if let Some(tag) = tag {
            format!("/repos/houseabsolute/ubi/releases/tags/{tag}")
        } else {
            "/repos/houseabsolute/ubi/releases/latest".to_string()
        };
        let authorization_header_matcher = {
            if let Some(token) = token {
                mockito::Matcher::Exact(format!("Bearer {token}"))
            } else {
                mockito::Matcher::Missing
            }
        };
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", expect_path.as_str())
            .match_header("Authorization", authorization_header_matcher)
            .with_status(200)
            .with_body(serde_json::to_string(&github::Release {
                assets: assets.clone(),
                published_at: chrono::Utc::now(),
            })?)
            .create_async()
            .await;

        let forge = ForgeType::GitHub.new_forge(
            "houseabsolute/ubi".to_string(),
            tag.map(String::from),
            Some(server.url()),
            token.map(String::from),
        )?;

        let client = Client::new();
        let got_assets = forge.fetch_assets(&client).await?;
        assert_eq!(got_assets, assets);

        m.assert_async().await;

        for (k, v) in vars {
            env::set_var(k, v);
        }

        Ok(())
    }

    #[test]
    fn github_api_base_url() -> Result<()> {
        let url = ForgeType::GitHub.release_info_url(
            "houseabsolute/ubi",
            Url::parse("https://github.example.com/api/v4")?,
            None,
        );
        assert_eq!(
            url.as_str(),
            "https://github.example.com/api/v4/repos/houseabsolute/ubi/releases/latest"
        );
        Ok(())
    }

    #[test(tokio::test)]
    #[serial]
    async fn gitlab_fetch_assets_without_token() -> Result<()> {
        gitlab_fetch_assets(None, None).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn gitlab_fetch_assets_with_token() -> Result<()> {
        gitlab_fetch_assets(None, Some("glpat-fakeToken")).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn gitlab_fetch_assets_with_tag() -> Result<()> {
        gitlab_fetch_assets(Some("v1.0.0"), None).await
    }

    async fn gitlab_fetch_assets(tag: Option<&str>, token: Option<&str>) -> Result<()> {
        let vars = env::vars();
        env::remove_var("GITLAB_TOKEN");
        env::remove_var("CI_JOB_TOKEN");
        env::remove_var("CODEBERG_TOKEN");
        env::remove_var("FORGEJO_TOKEN");

        let assets = vec![Asset {
            name: "asset1".to_string(),
            url: Url::parse("https://gitlab.com/api/v4/projects/owner%2Frepo/releases/assets/1")?,
        }];

        let expect_path = if let Some(tag) = tag {
            format!("/projects/houseabsolute%2Fubi/releases/{tag}")
        } else {
            "/projects/houseabsolute%2Fubi/releases/permalink/latest".to_string()
        };
        let authorization_header_matcher = {
            if let Some(token) = token {
                mockito::Matcher::Exact(format!("Bearer {token}"))
            } else {
                mockito::Matcher::Missing
            }
        };
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", expect_path.as_str())
            .match_header("Authorization", authorization_header_matcher)
            .with_status(200)
            .with_body(serde_json::to_string(&gitlab::Release {
                assets: gitlab::Assets {
                    links: assets.clone(),
                },
                released_at: chrono::Utc::now(),
            })?)
            .create_async()
            .await;

        let forge = ForgeType::GitLab.new_forge(
            "houseabsolute/ubi".to_string(),
            tag.map(String::from),
            Some(server.url()),
            token.map(String::from),
        )?;

        let client = Client::new();
        let got_assets = forge.fetch_assets(&client).await?;
        assert_eq!(got_assets, assets);

        m.assert_async().await;

        for (k, v) in vars {
            env::set_var(k, v);
        }

        Ok(())
    }

    #[test]
    fn gitlab_api_base_url() -> Result<()> {
        let url = ForgeType::GitLab.release_info_url(
            "houseabsolute/ubi",
            Url::parse("https://gitlab.example.com/api/v4")?,
            None,
        );
        assert_eq!(
            url.as_str(),
            "https://gitlab.example.com/api/v4/projects/houseabsolute%2Fubi/releases/permalink/latest"
        );
        Ok(())
    }
}
