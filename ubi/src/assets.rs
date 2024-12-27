use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};
use url::Url;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct Asset {
    pub(crate) name: String,
    pub(crate) url: Url,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Assets(HashMap<String, Url>);

impl From<Vec<Asset>> for Assets {
    fn from(assets: Vec<Asset>) -> Assets {
        Assets(assets.into_iter().map(|a| (a.name, a.url)).collect())
    }
}

impl FromIterator<Asset> for Assets {
    fn from_iter<I: IntoIterator<Item = Asset>>(iter: I) -> Self {
        Assets(iter.into_iter().map(|a| (a.name, a.url)).collect())
    }
}

impl Deref for Assets {
    type Target = HashMap<String, Url>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Assets {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
