use crate::{forge::Forge, ubi::Asset};
use anyhow::Result;
use async_trait::async_trait;
use itertools::Itertools;
use log::debug;
use reqwest::{header::HeaderValue, header::AUTHORIZATION, Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug)]
pub(crate) struct Forgejo {
    project_name: String,
    tag: Option<String>,
    api_base_url: Url,
    token: Option<String>,
}

unsafe impl Send for Forgejo {}
unsafe impl Sync for Forgejo {}

#[derive(Debug, Deserialize, Serialize)]
struct Release {
    assets: Vec<Attachment>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Attachment {
    name: String,
    #[serde(rename = "browser_download_url")]
    url: Url,
}

impl From<&Asset> for Attachment {
    fn from(a: &Asset) -> Attachment {
        Attachment {
            url: a.url.clone(),
            name: a.name.clone(),
        }
    }
}
impl Into<Asset> for Attachment {
    fn into(self) -> Asset {
        Asset {
            url: self.url.clone(),
            name: self.name.clone(),
        }
    }
}

#[async_trait]
impl Forge for Forgejo {
    async fn fetch_assets(&self, client: &Client) -> Result<Vec<Asset>> {
        Ok(self
            .make_release_info_request(client)
            .await?
            .json::<Release>()
            .await?
            .assets
            .into_iter()
            .map_into()
            .to_owned()
            .collect())
    }

    fn release_info_url(&self) -> Url {
        let mut url = self.api_base_url.clone();
        url.path_segments_mut()
            .expect("could not get path segments for url")
            .push("repos")
            .extend(self.project_name.split('/'))
            .push("releases");
        if let Some(tag) = &self.tag {
            url.path_segments_mut()
                .expect("could not get path segments for url")
                .push("tags")
                .push(tag);
        } else {
            url.path_segments_mut()
                .expect("could not get path segments for url")
                .extend(&["latest"]);
        }

        url
    }

    fn maybe_add_token_header(&self, mut req_builder: RequestBuilder) -> Result<RequestBuilder> {
        if let Some(token) = self.token.as_deref() {
            debug!("Adding Oauth2 token to Forgejo request.");
            let bearer = format!("Bearer {token}");
            let mut auth_val = HeaderValue::from_str(&bearer)?;
            auth_val.set_sensitive(true);
            req_builder = req_builder.header(AUTHORIZATION, auth_val);
        } else {
            debug!("No Forgejo token found.");
        }
        Ok(req_builder)
    }
}

impl Forgejo {
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
        fetch_assets(None, Some("eyFakeToken")).await
    }

    #[test(tokio::test)]
    #[serial]
    async fn fetch_assets_with_tag() -> Result<()> {
        fetch_assets(Some("v1.0.0"), None).await
    }

    async fn fetch_assets(tag: Option<&str>, token: Option<&str>) -> Result<()> {
        let vars = env::vars();
        env::remove_var("FORGEJO_TOKEN");

        let assets = vec![Asset {
            name: "asset1".to_string(),
            url: Url::parse("https://codeberg.org/api/v1/repos/owner/repo/releases/assets/1")?,
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
                assets: assets.iter().map(Attachment::from).collect(),
            })?)
            .create_async()
            .await;

        let forgejo = Forgejo::new(
            "houseabsolute/ubi".to_string(),
            tag.map(String::from),
            Url::parse(&server.url())?,
            token.map(String::from),
        );

        let client = Client::new();
        let got_assets = forgejo.fetch_assets(&client).await?;
        assert_eq!(got_assets, assets);

        m.assert_async().await;

        for (k, v) in vars {
            env::set_var(k, v);
        }

        Ok(())
    }

    #[test]
    fn api_base_url() {
        let forgejo = Forgejo::new(
            "houseabsolute/ubi".to_string(),
            None,
            Url::parse("https://forgejo.example.com/api/v1").unwrap(),
            None,
        );
        let url = forgejo.release_info_url();
        assert_eq!(
            url.as_str(),
            "https://forgejo.example.com/api/v1/repos/houseabsolute/ubi/releases/latest"
        );
    }
}
