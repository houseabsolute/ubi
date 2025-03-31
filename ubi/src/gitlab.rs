use crate::{forge::Forge, ubi::Asset};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::debug;
use reqwest::{header::HeaderValue, header::AUTHORIZATION, Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug)]
pub(crate) struct GitLab {
    project_name: String,
    tag: Option<String>,
    api_base_url: Url,
    token: Option<String>,
}

unsafe impl Send for GitLab {}
unsafe impl Sync for GitLab {}

#[derive(Debug, Deserialize, Serialize)]
struct Release {
    assets: GitLabAssets,
}

#[derive(Debug, Deserialize, Serialize)]
struct GitLabAssets {
    links: Vec<Asset>,
}

#[async_trait]
impl Forge for GitLab {
    async fn fetch_assets(&self, client: &Client) -> Result<Vec<Asset>> {
        Ok(self
            .make_release_info_request(client)
            .await?
            .json::<Release>()
            .await?
            .assets
            .links)
    }

    fn release_info_url(&self) -> Url {
        let mut url = self.api_base_url.clone();
        url.path_segments_mut()
            .expect("could not get path segments for url")
            .push("projects")
            .push(&self.project_name)
            .push("releases");
        if let Some(tag) = &self.tag {
            url.path_segments_mut()
                .expect("could not get path segments for url")
                .push(tag);
        } else {
            url.path_segments_mut()
                .expect("could not get path segments for url")
                .extend(&["permalink", "latest"]);
        }

        url
    }

    fn maybe_add_token_header(&self, mut req_builder: RequestBuilder) -> Result<RequestBuilder> {
        if let Some(token) = self.token.as_deref() {
            debug!("Adding GitLab token to GitLab request.");
            let bearer = format!("Bearer {token}");
            let mut auth_val = HeaderValue::from_str(&bearer)?;
            auth_val.set_sensitive(true);
            req_builder = req_builder.header(AUTHORIZATION, auth_val);
        } else {
            debug!("No GitLab token found.");
        }
        Ok(req_builder)
    }
}

impl GitLab {
    pub(crate) fn new(
        project_name: String,
        tag: Option<String>,
        api_base_url: Url,
        token: Option<String>,
    ) -> Self {
        Self {
            project_name,
            tag,
            api_base_url,
            token,
        }
    }

    pub(crate) fn parse_project_name_from_url(url: &Url, from: String) -> Result<String> {
        let mut parts = url.path().split('/').collect::<Vec<_>>();

        if parts.len() < 3 {
            return Err(anyhow!("could not parse project from {from}"));
        }

        // GitLab supports deeply nested projects (more than org/project)
        parts.remove(0);

        // Remove the trailing / if there is one
        if let Some(last) = parts.last() {
            if last.is_empty() {
                parts.pop();
            }
        }

        // Stop at the first `-` component, as this is GitLab's routing separator
        // and indicates we've moved beyond the project path
        if let Some(dash_pos) = parts.iter().position(|&s| s == "-") {
            parts.truncate(dash_pos);
        }

        if !parts.iter().all(|s| !s.is_empty()) {
            return Err(anyhow!("could not parse project from {from}"));
        }

        Ok(parts.join("/"))
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
    async fn fetch_assets_without_token() -> Result<()> {
        fetch_assets(None, None).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn fetch_assets_with_token() -> Result<()> {
        fetch_assets(None, Some("glpat-fakeToken")).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn fetch_assets_with_tag() -> Result<()> {
        fetch_assets(Some("v1.0.0"), None).await
    }

    async fn fetch_assets(tag: Option<&str>, token: Option<&str>) -> Result<()> {
        let vars = env::vars();
        env::remove_var("GITLAB_TOKEN");
        env::remove_var("CI_JOB_TOKEN");

        let assets = vec![Asset {
            name: "asset1".to_string(),
            url: Url::parse("https://gitlab.com/api/v4/projects/owner%2Frepo/releases/assets/1")?,
        }];

        let expect_path = if let Some(tag) = tag {
            format!("/projects/houseabsolute%2Fubi/releases/{tag}")
        } else {
            "/projects/houseabsolute%2Fubi/releases/permalink/latest".to_string()
        };
        let authorization_header_matcher = if token.is_some() {
            mockito::Matcher::Exact(format!("Bearer {}", token.unwrap()))
        } else {
            mockito::Matcher::Missing
        };
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", expect_path.as_str())
            .match_header("Authorization", authorization_header_matcher)
            .with_status(200)
            .with_body(serde_json::to_string(&Release {
                assets: GitLabAssets {
                    links: assets.clone(),
                },
            })?)
            .create_async()
            .await;

        let github = GitLab::new(
            "houseabsolute/ubi".to_string(),
            tag.map(String::from),
            Url::parse(&server.url())?,
            token.map(String::from),
        );

        let client = Client::new();
        let got_assets = github.fetch_assets(&client).await?;
        assert_eq!(got_assets, assets);

        m.assert_async().await;

        for (k, v) in vars {
            env::set_var(k, v);
        }

        Ok(())
    }

    #[test]
    fn api_base_url() {
        let gitlab = GitLab::new(
            "houseabsolute/ubi".to_string(),
            None,
            Url::parse("https://gitlab.example.com/api/v4").unwrap(),
            None,
        );
        let url = gitlab.release_info_url();
        assert_eq!(
            url.as_str(),
            "https://gitlab.example.com/api/v4/projects/houseabsolute%2Fubi/releases/permalink/latest"
        );
    }

    #[test]
    fn parse_project_name_from_url_basic() -> Result<()> {
        let url = Url::parse("https://gitlab.com/owner/repo")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "owner/repo");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_deeply_nested() -> Result<()> {
        let url = Url::parse("https://gitlab.com/gitlab-com/gl-infra/terra-transformer")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "gitlab-com/gl-infra/terra-transformer");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_very_deeply_nested() -> Result<()> {
        let url = Url::parse("https://gitlab.com/group/subgroup/subsubgroup/project")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "group/subgroup/subsubgroup/project");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_with_path() -> Result<()> {
        let url = Url::parse("https://gitlab.com/owner/repo/releases")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "owner/repo/releases");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_with_trailing_slash() -> Result<()> {
        let url = Url::parse("https://gitlab.com/owner/repo/")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "owner/repo");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_nested_with_trailing_slash() -> Result<()> {
        let url = Url::parse("https://gitlab.com/gitlab-com/gl-infra/terra-transformer/")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "gitlab-com/gl-infra/terra-transformer");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_complex_path() -> Result<()> {
        let url = Url::parse("https://gitlab.com/owner/repo/-/releases/tag/v1.0.0")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "owner/repo");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_error_too_short() {
        let url = Url::parse("https://gitlab.com/owner").unwrap();
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("could not parse project from test"));
    }

    #[test]
    fn parse_project_name_from_url_error_empty_segments() {
        let url = Url::parse("https://gitlab.com//repo").unwrap();
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("could not parse project from test"));
    }

    #[test]
    fn parse_project_name_from_url_error_root_only() {
        let url = Url::parse("https://gitlab.com/").unwrap();
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("could not parse project from test"));
    }

    #[test]
    fn parse_project_name_from_url_stops_at_dash() -> Result<()> {
        let url = Url::parse("https://gitlab.com/owner/repo/-/issues/123")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "owner/repo");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_nested_stops_at_dash() -> Result<()> {
        let url = Url::parse("https://gitlab.com/gitlab-com/gl-infra/terra-transformer/-/merge_requests/42")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "gitlab-com/gl-infra/terra-transformer");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_dash_in_project_name() -> Result<()> {
        // This tests that project names containing dashes are preserved
        let url = Url::parse("https://gitlab.com/my-org/my-project")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "my-org/my-project");
        Ok(())
    }

    #[test]
    fn parse_project_name_from_url_dash_separator_only() -> Result<()> {
        // Test URL that has a standalone `-` as a path separator
        let url = Url::parse("https://gitlab.com/group/project/-")?;
        let result = GitLab::parse_project_name_from_url(&url, "test".to_string())?;
        assert_eq!(result, "group/project");
        Ok(())
    }
}
