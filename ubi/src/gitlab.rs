use crate::release::{Asset, Release};
use anyhow::Result;
use mockito::Server;
use reqwest::{header::HeaderValue, header::ACCEPT, Client};
use url::Url;

#[derive(Debug)]
pub(crate) struct GitLabAssetFetcher {
    project_name: String,
    tag: Option<String>,
    url: Option<Url>,
    gitlab_api_base: String,
}

const GITLAB_API_BASE: &str = "https://gitlab.com/api/v4";

impl GitLabAssetFetcher {
    pub(crate) fn new(
        project_name: String,
        tag: Option<String>,
        url: Option<Url>,
        gitlab_api_base: Option<String>,
    ) -> Self {
        Self {
            project_name,
            tag,
            url,
            gitlab_api_base: gitlab_api_base.unwrap_or(GITLAB_API_BASE.to_string()),
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
        let url = match &self.tag {
            Some(tag) => format!(
                "{}/projects/{}/releases/{}",
                self.gitlab_api_base, self.project_name, tag
            ),
            None => format!(
                "{}/projects/{}/releases",
                self.gitlab_api_base, self.project_name
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

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;
    use reqwest::Client;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_fetch_assets_with_token() -> Result<()> {
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", "/projects/owner%2Frepo/releases")
            .match_header("Authorization", "Bearer test_token")
            .with_status(200)
            .with_body(
                r#"{
                    "assets": [
                        {
                            "name": "asset1",
                            "url": "https://gitlab.com/api/v4/projects/owner%2Frepo/releases/assets/1"
                        }
                    ]
                }"#,
            )
            .create_async()
            .await;

        let client = Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_static("Bearer test_token"),
                );
                headers
            })
            .build()?;

        let fetcher = GitLabAssetFetcher::new(
            "owner/repo".to_string(),
            None,
            None,
            Some(server.url().to_string()),
        );

        let assets = fetcher.fetch_assets(&client).await?;
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "asset1");

        m.assert_async().await;
        Ok(())
    }

    #[test(tokio::test)]
    async fn test_fetch_assets_with_tag() -> Result<()> {
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", "/projects/owner%2Frepo/releases/v1.0.0")
            .with_status(200)
            .with_body(
                r#"{
                    "assets": [
                        {
                            "name": "asset1",
                            "url": "https://gitlab.com/api/v4/projects/owner%2Frepo/releases/assets/1"
                        }
                    ]
                }"#,
            )
            .create_async()
            .await;

        let client = Client::new();
        let fetcher = GitLabAssetFetcher::new(
            "owner/repo".to_string(),
            Some("v1.0.0".to_string()),
            None,
            Some(server.url().to_string()),
        );

        let assets = fetcher.fetch_assets(&client).await?;
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "asset1");

        m.assert_async().await;
        Ok(())
    }

    #[test(tokio::test)]
    async fn test_fetch_assets_with_url() -> Result<()> {
        let client = Client::new();
        let fetcher = GitLabAssetFetcher::new(
            "owner/repo".to_string(),
            None,
            Some(Url::parse("https://gitlab.com/api/v4/projects/owner%2Frepo/releases/assets/1")?),
            None,
        );

        let assets = fetcher.fetch_assets(&client).await?;
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "1");
        assert_eq!(
            assets[0].url,
            Url::parse("https://gitlab.com/api/v4/projects/owner%2Frepo/releases/assets/1")?
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_fetch_assets_with_different_json_structure() -> Result<()> {
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", "/projects/owner%2Frepo/releases")
            .with_status(200)
            .with_body(
                r#"[{
                    "assets": {
                        "count": 6,
                        "links": [
                            {
                                "id": 2,
                                "name": "awesome-v0.2.msi",
                                "url": "http://192.168.10.15:3000/msi",
                                "link_type": "other"
                            },
                            {
                                "id": 1,
                                "name": "awesome-v0.2.dmg",
                                "url": "http://192.168.10.15:3000",
                                "link_type": "other"
                            }
                        ],
                        "evidence_file_path": "https://gitlab.example.com/root/awesome-app/-/releases/v0.2/evidence.json"
                    }
                }]"#,
            )
            .create_async()
            .await;

        let client = Client::new();
        let fetcher = GitLabAssetFetcher::new(
            "owner/repo".to_string(),
            None,
            None,
            Some(server.url().to_string()),
        );

        let assets = fetcher.fetch_assets(&client).await?;
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].name, "awesome-v0.2.msi");
        assert_eq!(assets[1].name, "awesome-v0.2.dmg");

        m.assert_async().await;
        Ok(())
    }
}
