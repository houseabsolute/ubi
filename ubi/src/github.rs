use crate::{
    assets::{Asset, Assets},
    forge::{Forge, ForgeType},
};
use anyhow::Result;
use async_trait::async_trait;
use log::debug;
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, RequestBuilder,
};
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

#[derive(Debug)]
pub(crate) struct GitHub {
    project_name: String,
    tag: Option<String>,
    api_base_url: Url,
    token: Option<String>,
}

unsafe impl Send for GitHub {}
unsafe impl Sync for GitHub {}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Release {
    pub(crate) assets: Vec<Asset>,
}

#[async_trait]
impl Forge for GitHub {
    async fn fetch_assets(&self, client: &Client) -> Result<Assets> {
        Ok(self
            .make_release_info_request(client)
            .await?
            .json::<Release>()
            .await?
            .assets
            .into_iter()
            .collect())
    }

    fn release_info_url(&self) -> Url {
        let mut parts = self.project_name.split('/');
        let owner = parts.next().unwrap();
        let repo = parts.next().unwrap();

        let mut url = self.api_base_url.clone();
        url.path_segments_mut()
            .expect("could not get path segments for url")
            .push("repos")
            .push(owner)
            .push(repo)
            .push("releases");
        if let Some(tag) = &self.tag {
            url.path_segments_mut()
                .expect("could not get path segments for url")
                .push("tags")
                .push(tag);
        } else {
            url.path_segments_mut()
                .expect("could not get path segments for url")
                .push("latest");
        }

        url
    }

    fn maybe_add_token_header(&self, mut req_builder: RequestBuilder) -> Result<RequestBuilder> {
        if let Some(token) = self.token.as_deref() {
            debug!("Adding GitHub token to GitHub request.");
            let bearer = format!("Bearer {token}");
            let mut auth_val = HeaderValue::from_str(&bearer)?;
            auth_val.set_sensitive(true);
            req_builder = req_builder.header(AUTHORIZATION, auth_val);
        }
        Ok(req_builder)
    }
}

impl GitHub {
    pub(crate) fn new(
        project_name: String,
        tag: Option<String>,
        api_base: Option<Url>,
        token: Option<&str>,
    ) -> Self {
        let mut token = token.map(String::from);
        if token.is_none() {
            token = env::var("GITHUB_TOKEN").ok();
        }

        Self {
            project_name,
            tag,
            api_base_url: api_base.unwrap_or_else(|| ForgeType::GitHub.api_base()),
            token,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use reqwest::Client;
    use serial_test::serial;
    use test_log::test;

    #[test(tokio::test)]
    #[serial]
    async fn fetch_assets_without_token() -> Result<()> {
        fetch_assets(None, None).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn fetch_assets_with_token() -> Result<()> {
        fetch_assets(None, Some("ghp_fakeToken")).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn fetch_assets_with_tag() -> Result<()> {
        fetch_assets(Some("v1.0.0"), None).await
    }

    async fn fetch_assets(tag: Option<&str>, token: Option<&str>) -> Result<()> {
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
                assets: assets.clone(),
            })?)
            .create_async()
            .await;

        let github = GitHub::new(
            "houseabsolute/ubi".to_string(),
            tag.map(String::from),
            Some(Url::parse(&server.url())?),
            token,
        );

        let client = Client::new();
        let got_assets = github.fetch_assets(&client).await?;
        assert_eq!(got_assets, assets.into());

        m.assert_async().await;

        for (k, v) in vars {
            env::set_var(k, v);
        }

        Ok(())
    }

    #[test]
    fn api_base_url() {
        let github = GitHub::new(
            "houseabsolute/ubi".to_string(),
            None,
            Some(Url::parse("https://github.example.com/api/v4").unwrap()),
            None,
        );
        let url = github.release_info_url();
        assert_eq!(
            url.as_str(),
            "https://github.example.com/api/v4/repos/houseabsolute/ubi/releases/latest"
        );
    }
}
