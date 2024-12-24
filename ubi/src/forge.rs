use crate::ubi::Asset;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::{
    header::{HeaderValue, ACCEPT},
    Client, RequestBuilder, Response,
};
// It'd be nice to use clap::ValueEnum here, but then we'd need to add clap as a dependency for the
// library code, which would be annoying for downstream users who just want to use the library.
use strum::{AsRefStr, EnumString, VariantNames};
use url::Url;

#[derive(AsRefStr, Clone, Debug, Default, EnumString, PartialEq, Eq, VariantNames)]
#[allow(clippy::module_name_repetitions)]
pub enum ForgeType {
    #[strum(serialize = "github")]
    #[default]
    GitHub,
    #[strum(serialize = "gitlab")]
    GitLab,
}

#[async_trait]
pub(crate) trait Forge: std::fmt::Debug {
    async fn fetch_assets(&self, client: &Client) -> Result<Vec<Asset>>;

    fn release_info_url(&self) -> Url;
    fn maybe_add_token_header(&self, req_builder: RequestBuilder) -> Result<RequestBuilder>;

    async fn make_release_info_request(&self, client: &Client) -> Result<Response> {
        let mut req_builder = client
            .get(self.release_info_url())
            .header(ACCEPT, HeaderValue::from_str("application/json")?);
        req_builder = self.maybe_add_token_header(req_builder)?;
        let resp = client.execute(req_builder.build()?).await?;

        if let Err(e) = resp.error_for_status_ref() {
            return Err(anyhow::Error::new(e));
        }

        Ok(resp)
    }
}

const GITHUB_DOMAIN: &str = "github.com";
const GITLAB_DOMAIN: &str = "gitlab.com";

const GITHUB_API_BASE: &str = "https://api.github.com";
const GITLAB_API_BASE: &str = "https://gitlab.com/api/v4";

impl ForgeType {
    pub(crate) fn from_url(url: &Url) -> ForgeType {
        if url.domain().unwrap().contains(GITLAB_DOMAIN) {
            ForgeType::GitLab
        } else {
            ForgeType::default()
        }
    }

    pub(crate) fn url_base(&self) -> Url {
        match self {
            ForgeType::GitHub => Url::parse(&format!("https://{GITHUB_DOMAIN}")).unwrap(),
            ForgeType::GitLab => Url::parse(&format!("https://{GITLAB_DOMAIN}")).unwrap(),
        }
    }

    pub(crate) fn api_base(&self) -> Url {
        match self {
            ForgeType::GitHub => Url::parse(GITHUB_API_BASE).unwrap(),
            ForgeType::GitLab => Url::parse(GITLAB_API_BASE).unwrap(),
        }
    }
}
