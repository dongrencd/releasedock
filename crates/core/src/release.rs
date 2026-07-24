use std::{fs, io::Write, path::Path};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{
    Proxy,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT},
    StatusCode,
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
        headers.insert(USER_AGENT, HeaderValue::from_static("releasedock"));
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
        self.latest_release_optional(repo)
            .await?
            .ok_or_else(|| anyhow::anyhow!("GitHub latest release request returned 404 Not Found"))
    }

    pub async fn latest_release_optional(&self, repo: &RepoRef) -> Result<Option<Release>> {
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

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let error = github_response_error("GitHub latest release", response).await;
            anyhow::bail!(error);
        }

        response
            .json::<Release>()
            .await
            .context("failed to parse GitHub release response")
            .map(Some)
    }

    pub async fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("failed to request GitHub asset")?;

        if !response.status().is_success() {
            let error = github_response_error("GitHub asset", response).await;
            anyhow::bail!(error);
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .context("failed to read GitHub asset body")
    }

    pub async fn download_to_path<F>(&self, url: &str, path: &Path, mut on_progress: F) -> Result<()>
    where
        F: FnMut(u64, Option<u64>),
    {
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .context("failed to request GitHub asset")?;

        if !response.status().is_success() {
            let error = github_response_error("GitHub asset", response).await;
            anyhow::bail!(error);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create download directory {}", parent.display()))?;
        }

        let mut file = fs::File::create(path)
            .with_context(|| format!("failed to create download {}", path.display()))?;
        let total = response.content_length();
        let mut downloaded = 0u64;
        on_progress(downloaded, total);

        while let Some(chunk) = response
            .chunk()
            .await
            .context("failed to read GitHub asset body")?
        {
            file.write_all(&chunk)
                .with_context(|| format!("failed to write download {}", path.display()))?;
            downloaded += chunk.len() as u64;
            on_progress(downloaded, total);
        }

        file.flush()
            .with_context(|| format!("failed to flush download {}", path.display()))?;
        Ok(())
    }
}

async fn github_response_error(resource: &str, response: reqwest::Response) -> String {
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.text().await.unwrap_or_default();
    format_github_response_error(resource, status, &headers, &body)
}

fn format_github_response_error(
    resource: &str,
    status: StatusCode,
    headers: &HeaderMap,
    body: &str,
) -> String {
    let mut message = format!("{resource} request failed with {status}");
    if let Some(summary) = summarize_github_body(body) {
        message.push_str(": ");
        message.push_str(&summary);
    }

    let mut extras = Vec::new();
    if let Some(request_id) = header_value(headers, "x-github-request-id") {
        extras.push(format!("request id {request_id}"));
    }
    if let Some(remaining) = header_value(headers, "x-ratelimit-remaining") {
        extras.push(format!("rate limit remaining {remaining}"));
    }
    if let Some(reset) = header_value(headers, "x-ratelimit-reset") {
        extras.push(format!("rate limit reset {reset}"));
    }
    if let Some(retry_after) = headers.get(RETRY_AFTER).and_then(|value| value.to_str().ok()) {
        extras.push(format!("retry after {retry_after}"));
    }

    if !extras.is_empty() {
        message.push_str(" (");
        message.push_str(&extras.join(", "));
        message.push(')');
    }

    message
}

fn summarize_github_body(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let mut parts = Vec::new();
        if let Some(message) = value.get("message").and_then(|value| value.as_str()) {
            let message = message.trim();
            if !message.is_empty() {
                parts.push(message.to_string());
            }
        }
        if let Some(documentation_url) = value.get("documentation_url").and_then(|value| value.as_str()) {
            let documentation_url = documentation_url.trim();
            if !documentation_url.is_empty() {
                parts.push(documentation_url.to_string());
            }
        }
        if !parts.is_empty() {
            return Some(parts.join(" | "));
        }
    }

    Some(trimmed.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" "))
        .filter(|text| !text.is_empty())
        .map(|text| shorten(&text, 240))
}

fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }

    let mut shortened = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index + 1 >= limit {
            break;
        }
        shortened.push(ch);
    }
    shortened.push_str("...");
    shortened
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::format_github_response_error;
    use reqwest::{
        StatusCode,
        header::{HeaderMap, HeaderValue},
    };

    #[test]
    fn formats_github_error_with_body_and_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-github-request-id", HeaderValue::from_static("abc123"));
        headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        headers.insert("x-ratelimit-reset", HeaderValue::from_static("1234567890"));
        headers.insert("retry-after", HeaderValue::from_static("60"));

        let message = format_github_response_error(
            "GitHub latest release",
            StatusCode::FORBIDDEN,
            &headers,
            r#"{"message":"Request forbidden by administrative rules.","documentation_url":"https://docs.github.com/rest/releases/releases#get-the-latest-release"}"#,
        );

        assert!(message.contains("GitHub latest release request failed with 403 Forbidden"));
        assert!(message.contains("Request forbidden by administrative rules."));
        assert!(message.contains("https://docs.github.com/rest/releases/releases#get-the-latest-release"));
        assert!(message.contains("request id abc123"));
        assert!(message.contains("rate limit remaining 0"));
        assert!(message.contains("rate limit reset 1234567890"));
        assert!(message.contains("retry after 60"));
    }

    #[test]
    fn shortens_plain_text_body() {
        let mut headers = HeaderMap::new();
        headers.insert("x-github-request-id", HeaderValue::from_static("abc123"));

        let body = "line one\n\nline two ".repeat(40);
        let message = format_github_response_error("GitHub asset", StatusCode::BAD_GATEWAY, &headers, &body);

        assert!(message.starts_with("GitHub asset request failed with 502 Bad Gateway"));
        assert!(message.contains("line one"));
        assert!(message.contains("request id abc123"));
        assert!(message.len() < 600);
    }
}
