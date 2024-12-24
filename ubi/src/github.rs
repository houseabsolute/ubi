use crate::{
    forge::{Forge, ForgeType},
    ubi::Asset,
};
use anyhow::Result;
use async_trait::async_trait;
use log::debug;
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, RequestBuilder,
};
use serde::Deserialize;
use std::env;
use url::Url;

#[derive(Debug)]
pub(crate) struct GitHub {
    project_name: String,
    tag: Option<String>,
    api_base: Url,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Release {
    pub(crate) assets: Vec<Asset>,
}

#[async_trait]
impl Forge for GitHub {
    async fn fetch_assets(&self, client: &Client) -> Result<Vec<Asset>> {
        Ok(self
            .make_release_info_request(client)
            .await?
            .json::<Release>()
            .await?
            .assets)
    }

    fn release_info_url(&self) -> Url {
        let mut parts = self.project_name.split('/');
        let owner = parts.next().unwrap();
        let repo = parts.next().unwrap();

        let mut url = self.api_base.clone();
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
            api_base: api_base.unwrap_or_else(|| ForgeType::GitHub.api_base()),
            token,
        }
    }
}
