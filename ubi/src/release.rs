use serde::Deserialize;
use std::path::PathBuf;
use tempfile::TempDir;
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

#[derive(Debug)]
pub(crate) struct Download {
    // We need to keep the temp dir around so that it's not deleted before
    // we're done with it.
    pub(crate) _temp_dir: TempDir,
    pub(crate) archive_path: PathBuf,
}
