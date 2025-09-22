use crate::ubi::Asset;
use anyhow::{anyhow, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use url::Url;

pub(crate) static PROJECT_BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://gitlab.com").unwrap());

pub(crate) static DEFAULT_API_BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://gitlab.com/api/v4").unwrap());

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Release {
    pub(crate) assets: Assets,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Assets {
    pub(crate) links: Vec<Asset>,
}

pub(crate) fn parse_project_name_from_url(url: &Url, from: &str) -> Result<String> {
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

    if parts.iter().any(|s| s.is_empty()) {
        return Err(anyhow!("could not parse project from {from}"));
    }

    debug!("Parsed {url} = {} / {}", parts[0], parts[1..].join("/"));

    Ok(parts.join("/"))
}

pub(crate) fn release_info_url(project_name: &str, mut url: Url, tag: Option<&str>) -> Url {
    url.path_segments_mut()
        .expect("could not get path segments for url")
        .push("projects")
        .push(project_name)
        .push("releases");
    if let Some(tag) = tag {
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
        "https://gitlab.com/owner/repo",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::nested_project_path(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer")
    )]
    #[case::deeply_nested_project_path(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/foo/bar",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer/foo/bar")
    )]
    #[case::with_trailing_slash(
        "https://gitlab.com/owner/repo/",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::nested_project_path_with_trailing_slash(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer")
    )]
    #[case::deeply_nested_project_path_with_trailing_slash(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/foo/bar/",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer/foo/bar")
    )]
    #[case::with_release_tag_in_path(
        "https://gitlab.com/owner/repo/-/releases/tag/v1.0.0",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::nested_with_release_tag_in_path(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/-/releases/tag/v1.0.0",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer")
    )]
    #[case::deeply_nested_with_release_tag_in_path(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/foo/bar/-/releases/tag/v1.0.0",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer/foo/bar")
    )]
    #[case::ends_in_dash(
        "https://gitlab.com/owner/repo/-",
        ParseTestExpect::Success("owner/repo")
    )]
    #[case::nested_ends_in_dash(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/-",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer")
    )]
    #[case::deeply_nested_ends_in_dash(
        "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/foo/bar/-",
        ParseTestExpect::Success("gitlab-com/gl-infra/terra-transformer/foo/bar")
    )]
    #[case::with_org_but_no_project(
        "https://gitlab.com/owner",
        ParseTestExpect::Fail("could not parse project from test")
    )]
    #[case::with_empty_path_segments(
        "https://gitlab.com/owner//repo",
        ParseTestExpect::Fail("could not parse project from test")
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
