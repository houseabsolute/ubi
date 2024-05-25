use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
pub(crate) struct Release {
    pub(crate) assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Asset {
    pub(crate) name: String,
    pub(crate) url: Url,
}
