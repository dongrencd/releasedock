use std::{fs, io::Write, path::Path, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{
    Proxy, StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT},
};
use serde::{Deserialize, Serialize};

use crate::repo::RepoRef;

const GITHUB_API_TIMEOUT: Duration = Duration::from_secs(20);
const GITHUB_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const GITHUB_DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(60);

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
    api_timeout: Duration,
}

impl ReleaseClient {
    pub fn new(github_token: Option<&str>, proxy_url: Option<&str>) -> Result<Self> {
        Self::with_timeouts(
            github_token,
            proxy_url,
            GITHUB_API_TIMEOUT,
            GITHUB_DOWNLOAD_READ_TIMEOUT,
        )
    }

    fn with_timeouts(
        github_token: Option<&str>,
        proxy_url: Option<&str>,
        api_timeout: Duration,
        download_read_timeout: Duration,
    ) -> Result<Self> {
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

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(GITHUB_CONNECT_TIMEOUT)
            .read_timeout(download_read_timeout);
        if let Some(proxy_url) = proxy_url.filter(|value| !value.trim().is_empty()) {
            let proxy =
                Proxy::all(proxy_url).context("failed to configure proxy for GitHub client")?;
            builder = builder.proxy(proxy);
        }

        Ok(Self {
            client: builder.build().context("failed to build GitHub client")?,
            api_timeout,
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
            .timeout(self.api_timeout)
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
            .timeout(self.api_timeout)
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

    pub async fn download_to_path<F>(
        &self,
        url: &str,
        path: &Path,
        mut on_progress: F,
    ) -> Result<()>
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
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create download directory {}", parent.display())
            })?;
        }

        let mut file = fs::File::create(path)
            .with_context(|| format!("failed to create download {}", path.display()))?;
        let total = response.content_length();
        let mut downloaded = 0u64;
        on_progress(downloaded, total);

        while let Some(chunk) = response
            .chunk()
            .await
            .context("failed to read GitHub asset body while downloading")?
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
    if let Some(retry_after) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    {
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
        if let Some(documentation_url) = value
            .get("documentation_url")
            .and_then(|value| value.as_str())
        {
            let documentation_url = documentation_url.trim();
            if !documentation_url.is_empty() {
                parts.push(documentation_url.to_string());
            }
        }
        if !parts.is_empty() {
            return Some(parts.join(" | "));
        }
    }

    Some(
        trimmed
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    )
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
    use std::time::Duration;

    use super::{ReleaseClient, format_github_response_error};
    use crate::repo::RepoRef;
    use reqwest::{
        StatusCode,
        header::{HeaderMap, HeaderValue},
    };
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener, time::sleep};

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
        assert!(
            message
                .contains("https://docs.github.com/rest/releases/releases#get-the-latest-release")
        );
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
        let message =
            format_github_response_error("GitHub asset", StatusCode::BAD_GATEWAY, &headers, &body);

        assert!(message.starts_with("GitHub asset request failed with 502 Bad Gateway"));
        assert!(message.contains("line one"));
        assert!(message.contains("request id abc123"));
        assert!(message.len() < 600);
    }

    #[tokio::test]
    async fn times_out_when_proxy_does_not_reply() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0u8; 1024];
                let mut received = Vec::new();

                loop {
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break;
                    }

                    received.extend_from_slice(&buffer[..read]);
                    if received.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }

                sleep(Duration::from_secs(5)).await;
            }
        });

        let proxy_url = format!("http://{proxy_addr}");
        let api_timeout = Duration::from_millis(100);
        let client = ReleaseClient::with_timeouts(
            None,
            Some(proxy_url.as_str()),
            api_timeout,
            Duration::from_millis(300),
        )
        .unwrap();
        let repo = RepoRef::parse("owner/project").unwrap();

        let error = client.latest_release_optional(&repo).await.unwrap_err();
        let is_timeout = error.chain().any(|cause| {
            cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|request_error| request_error.is_timeout())
        });
        assert!(is_timeout, "{error:#}");

        server.abort();
    }

    #[tokio::test]
    async fn downloads_asset_when_chunks_keep_arriving_before_read_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0u8; 1024];
                let mut received = Vec::new();

                loop {
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break;
                    }

                    received.extend_from_slice(&buffer[..read]);
                    if received.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }

                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/octet-stream\r\n\r\n",
                    )
                    .await
                    .unwrap();
                stream.flush().await.unwrap();

                stream.write_all(b"5\r\nhello\r\n").await.unwrap();
                stream.flush().await.unwrap();
                sleep(Duration::from_millis(120)).await;

                stream.write_all(b"1\r\n \r\n").await.unwrap();
                stream.flush().await.unwrap();
                sleep(Duration::from_millis(120)).await;

                stream.write_all(b"5\r\nworld\r\n").await.unwrap();
                stream.flush().await.unwrap();
                stream.write_all(b"0\r\n\r\n").await.unwrap();
                stream.flush().await.unwrap();
            }
        });

        let client = ReleaseClient::with_timeouts(
            None,
            None,
            Duration::from_millis(500),
            Duration::from_millis(200),
        )
        .unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");

        client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap();

        let bytes = std::fs::read(&download_path).unwrap();
        assert_eq!(bytes, b"hello world");

        server.abort();
    }

    #[tokio::test]
    async fn times_out_when_asset_body_stalls() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0u8; 1024];
                let mut received = Vec::new();

                loop {
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break;
                    }

                    received.extend_from_slice(&buffer[..read]);
                    if received.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }

                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/octet-stream\r\n\r\n",
                    )
                    .await
                    .unwrap();
                stream.flush().await.unwrap();

                stream.write_all(b"5\r\nhello\r\n").await.unwrap();
                stream.flush().await.unwrap();
                sleep(Duration::from_millis(300)).await;
                stream.write_all(b"5\r\nworld\r\n").await.unwrap();
                stream.flush().await.unwrap();
                stream.write_all(b"0\r\n\r\n").await.unwrap();
                stream.flush().await.unwrap();
            }
        });

        let client = ReleaseClient::with_timeouts(
            None,
            None,
            Duration::from_millis(500),
            Duration::from_millis(100),
        )
        .unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");

        let error = client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("while downloading"),
            "{error:#}"
        );
        let is_timeout = error.chain().any(|cause| {
            cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|request_error| request_error.is_timeout())
        });
        assert!(is_timeout, "{error:#}");

        server.abort();
    }
}
