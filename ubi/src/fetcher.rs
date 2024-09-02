use crate::release::{Asset, Release};
use anyhow::Result;
use reqwest::{header::HeaderValue, header::ACCEPT, Client};
use url::Url;

#[derive(Debug)]
pub(crate) struct GitHubAssetFetcher {
    project_name: String,
    tag: Option<String>,
    url: Option<Url>,
    github_api_base: String,
}

const GITHUB_API_BASE: &str = "https://api.github.com";

impl GitHubAssetFetcher {
    pub(crate) fn new(
        project_name: String,
        tag: Option<String>,
        url: Option<Url>,
        github_api_base: Option<String>,
    ) -> Self {
        Self {
            project_name,
            tag,
            url,
            github_api_base: github_api_base.unwrap_or(GITHUB_API_BASE.to_string()),
        }
    }

    pub(crate) async fn fetch_assets(&self, client: &Client) -> Result<Vec<Asset>> {
        if let Some(url) = &self.url {
            return Ok(vec![Asset {
                name: url.path().split('/').last().unwrap().to_string(),
                url: url.clone(),
            }]);
        }

        Ok(self.release_info(client).await?.assets)
    }

    async fn release_info(&self, client: &Client) -> Result<Release> {
        let mut parts = self.project_name.split('/');
        let owner = parts.next().unwrap();
        let repo = parts.next().unwrap();

        let url = match &self.tag {
            Some(tag) => format!(
                "{}/repos/{owner}/{repo}/releases/tags/{tag}",
                self.github_api_base,
            ),
            None => format!(
                "{}/repos/{owner}/{repo}/releases/latest",
                self.github_api_base,
            ),
        };
        let req = client
            .get(url)
            .header(ACCEPT, HeaderValue::from_str("application/json")?)
            .build()?;
        let resp = client.execute(req).await?;

        if let Err(e) = resp.error_for_status_ref() {
            return Err(anyhow::Error::new(e));
        }

        Ok(resp.json::<Release>().await?)
    }
}
