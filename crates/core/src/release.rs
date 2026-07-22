use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{
    Proxy,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT},
};
use serde::{Deserialize, Serialize};

use crate::repo::RepoRef;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(default)]
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

impl Release {
    pub fn fixture(tag_name: &str, assets: Vec<ReleaseAsset>) -> Self {
        Self {
            tag_name: tag_name.to_string(),
            name: Some(tag_name.to_string()),
            body: Some("Fixture release notes".to_string()),
            html_url: Some(format!(
                "https://github.com/example/example/releases/tag/{tag_name}"
            )),
            published_at: None,
            prerelease: false,
            assets,
        }
    }

    pub fn release_note(&self) -> Option<&str> {
        self.body
            .as_deref()
            .map(str::trim)
            .filter(|body| !body.is_empty())
    }
}

impl ReleaseAsset {
    pub fn fixture(name: &str) -> Self {
        Self {
            name: name.to_string(),
            browser_download_url: format!(
                "https://github.com/example/example/releases/download/v0/{name}"
            ),
            size: 0,
        }
    }
}

#[derive(Clone)]
pub struct ReleaseClient {
    client: reqwest::Client,
}

impl ReleaseClient {
    pub fn new(github_token: Option<&str>, proxy_url: Option<&str>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("gh-release-manager"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );

        if let Some(token) = github_token.filter(|token| !token.trim().is_empty()) {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .context("failed to build GitHub authorization header")?;
            headers.insert(AUTHORIZATION, value);
        }

        let mut builder = reqwest::Client::builder().default_headers(headers);
        if let Some(proxy_url) = proxy_url.filter(|value| !value.trim().is_empty()) {
            let proxy =
                Proxy::all(proxy_url).context("failed to configure proxy for GitHub client")?;
            builder = builder.proxy(proxy);
        }

        Ok(Self {
            client: builder.build().context("failed to build GitHub client")?,
        })
    }

    pub async fn latest_release(&self, repo: &RepoRef) -> Result<Release> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            repo.owner, repo.name
        );

        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("failed to request latest GitHub release")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "GitHub latest release request failed with {}",
                response.status()
            );
        }

        response
            .json::<Release>()
            .await
            .context("failed to parse GitHub release response")
    }

    pub async fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("failed to request GitHub asset")?;

        if !response.status().is_success() {
            anyhow::bail!("GitHub asset request failed with {}", response.status());
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .context("failed to read GitHub asset body")
    }
}
