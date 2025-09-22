use crate::ubi::Asset;
use anyhow::{anyhow, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use url::Url;

pub(crate) static PROJECT_BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://github.com").unwrap());

pub(crate) static DEFAULT_API_BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://api.github.com").unwrap());

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Release {
    pub(crate) assets: Vec<Asset>,
}

pub(crate) fn parse_project_name_from_url(url: &Url, from: &str) -> Result<String> {
    let parts = url.path().split('/').collect::<Vec<_>>();

    if parts.len() < 3 {
        return Err(anyhow!("could not parse project from {from}"));
    }

    if parts[1].is_empty() || parts[2].is_empty() {
        return Err(anyhow!("could not parse org and repo name from {from}"));
    }

    // The first part is an empty string for the leading '/' in the path.
    let (org, proj) = (parts[1], parts[2]);
    debug!("Parsed {url} = {org} / {proj}");

    Ok(format!("{org}/{proj}"))
}

pub(crate) fn release_info_url(project_name: &str, mut url: Url, tag: Option<&str>) -> Url {
    let mut parts = project_name.split('/');
    let owner = parts.next().unwrap();
    let repo = parts.next().unwrap();

    url.path_segments_mut()
        .expect("could not get path segments for url")
        .push("repos")
        .push(owner)
        .push(repo)
        .push("releases");
    if let Some(tag) = &tag {
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    enum ParseTestExpect {
        Success(&'static str),
        Fail(&'static str),
    }

    #[rstest]
    #[case::basic(
        "https://github.com/owner/repo",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::with_release_in_path(
        "https://github.com/owner/repo/releases",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::with_trailing_slash(
        "https://github.com/owner/repo/",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::with_release_tag_in_path(
        "https://github.com/owner/repo/releases/tag/v1.0.0",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::with_org_but_no_project(
        "https://github.com/owner",
        ParseTestExpect::Fail("could not parse project from test")
    )]
    #[case::with_empty_path_segments(
        "https://github.com/owner//repo",
        ParseTestExpect::Fail("could not parse org and repo name from test")
    )]
    fn parse_project_name(
        #[case] url: &'static str,
        #[case] expect: ParseTestExpect,
    ) -> Result<()> {
        let url = Url::parse(url)?;
        let result = super::parse_project_name_from_url(&url, "test");
        match (result, expect) {
            (Ok(r), ParseTestExpect::Success(e)) => assert_eq!(r, e),
            (Err(r), ParseTestExpect::Fail(e)) => assert!(r.to_string().contains(e)),
            (Ok(r), ParseTestExpect::Fail(e)) => {
                panic!("Expected failure {e} but got success: {r}")
            }
            (Err(r), ParseTestExpect::Success(e)) => {
                panic!("Expected success {e} but got failure: {r}")
            }
        }
        Ok(())
    }
}
