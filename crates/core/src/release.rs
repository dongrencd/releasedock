use std::{fs, io::Write, path::Path, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{
    Proxy, StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, LINK, RETRY_AFTER, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::repo::RepoRef;

const GITHUB_API_TIMEOUT: Duration = Duration::from_secs(20);
const GITHUB_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const GITHUB_DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(60);
const GITHUB_API_BASE_URL: &str = "https://api.github.com";

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
    pub draft: bool,
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
            draft: false,
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

/// GitHub Release 的单页响应以及服务端是否声明了下一页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePage {
    pub releases: Vec<Release>,
    pub has_next_page: bool,
}

#[derive(Clone)]
pub struct ReleaseClient {
    client: reqwest::Client,
    api_timeout: Duration,
    api_base_url: Url,
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
        let api_base_url = configured_api_base_url()?;
        Self::build(
            github_token,
            proxy_url,
            api_timeout,
            download_read_timeout,
            api_base_url,
        )
    }

    fn build(
        github_token: Option<&str>,
        proxy_url: Option<&str>,
        api_timeout: Duration,
        download_read_timeout: Duration,
        api_base_url: Url,
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
            api_base_url,
        })
    }

    /// 只在本模块 HTTP 单测中替换 API 地址，生产构建不会暴露该入口。
    #[cfg(test)]
    fn with_test_api_base_url(api_base_url: &str) -> Result<Self> {
        Self::build(
            None,
            None,
            GITHUB_API_TIMEOUT,
            GITHUB_DOWNLOAD_READ_TIMEOUT,
            Url::parse(api_base_url).context("failed to parse test GitHub API base URL")?,
        )
    }

    pub async fn latest_release(&self, repo: &RepoRef) -> Result<Release> {
        self.latest_release_optional(repo)
            .await?
            .ok_or_else(|| anyhow::anyhow!("GitHub latest release request returned 404 Not Found"))
    }

    pub async fn latest_release_optional(&self, repo: &RepoRef) -> Result<Option<Release>> {
        let url = self.api_url(repo, &["releases", "latest"])?;

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

    /// 获取 GitHub Release 的一页，`per_page` 会被约束到 GitHub 支持的 1..=100。
    pub async fn releases_page(
        &self,
        repo: &RepoRef,
        page: u32,
        per_page: u32,
    ) -> Result<ReleasePage> {
        let mut url = self.api_url(repo, &["releases"])?;
        url.query_pairs_mut()
            .append_pair("page", &page.max(1).to_string())
            .append_pair("per_page", &per_page.clamp(1, 100).to_string());

        let response = self
            .client
            .get(url)
            .timeout(self.api_timeout)
            .send()
            .await
            .context("failed to request GitHub releases")?;

        if !response.status().is_success() {
            let error = github_response_error("GitHub releases", response).await;
            anyhow::bail!(error);
        }

        let has_next_page = has_next_link(response.headers());
        let releases = response
            .json::<Vec<Release>>()
            .await
            .context("failed to parse GitHub releases response")?;
        Ok(ReleasePage {
            releases,
            has_next_page,
        })
    }

    /// 按完整 tag 查询 Release；GitHub 返回 404 时用 `None` 表示不存在。
    pub async fn release_by_tag(&self, repo: &RepoRef, tag: &str) -> Result<Option<Release>> {
        let url = self.api_url(repo, &["releases", "tags", tag])?;
        let response = self
            .client
            .get(url)
            .timeout(self.api_timeout)
            .send()
            .await
            .context("failed to request GitHub release by tag")?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let error = github_response_error("GitHub release by tag", response).await;
            anyhow::bail!(error);
        }

        response
            .json::<Release>()
            .await
            .context("failed to parse GitHub release by tag response")
            .map(Some)
    }

    /// 轻量检查 GitHub API 是否可达；不依赖具体仓库，适合设置页验证 token/proxy。
    pub async fn check_connectivity(&self) -> Result<()> {
        let mut url = self.api_base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("GitHub API base URL cannot contain path segments"))?;
            segments.pop_if_empty();
            segments.extend(["rate_limit"]);
        }

        let response = self
            .client
            .get(url)
            .timeout(self.api_timeout)
            .send()
            .await
            .context("failed to request GitHub connectivity check")?;

        if !response.status().is_success() {
            let error = github_response_error("GitHub connectivity check", response).await;
            anyhow::bail!(error);
        }

        Ok(())
    }

    fn api_url(&self, repo: &RepoRef, trailing_segments: &[&str]) -> Result<Url> {
        let mut url = self.api_base_url.clone();
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("GitHub API base URL cannot contain path segments"))?;
        segments.pop_if_empty();
        segments.extend(["repos", repo.owner.as_str(), repo.name.as_str()]);
        // push/extend 会把 tag 中的斜杠编码，保证 tag 始终是单个 path segment。
        segments.extend(trailing_segments.iter().copied());
        drop(segments);
        Ok(url)
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

    /// Reads an asset incrementally and stops as soon as the body exceeds the
    /// caller-provided limit, including for responses without Content-Length.
    pub async fn download_bytes_limited(&self, url: &str, max: u64) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .timeout(self.api_timeout)
            .send()
            .await
            .context("failed to request GitHub asset")?;

        if !response.status().is_success() {
            let error = github_response_error_limited("GitHub asset", response, max).await;
            anyhow::bail!(error);
        }
        read_response_bytes_limited(response, max).await
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

fn configured_api_base_url() -> Result<Url> {
    // This override is compiled out of release binaries. It exists only so a
    // debug CLI subprocess can exercise the complete HTTP wiring against a
    // local deterministic server rather than exposing a production setting.
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("RELEASEDOCK_TEST_GITHUB_API_BASE_URL")
        && !value.trim().is_empty()
    {
        return Url::parse(&value).context("failed to parse test GitHub API base URL");
    }

    Url::parse(GITHUB_API_BASE_URL).context("failed to parse built-in GitHub API base URL")
}

fn has_next_link(headers: &HeaderMap) -> bool {
    headers.get_all(LINK).iter().any(|value| {
        value.to_str().ok().is_some_and(|value| {
            value.split(',').any(|entry| {
                entry
                    .split(';')
                    .skip(1)
                    .any(|parameter| parameter.trim() == r#"rel="next""#)
            })
        })
    })
}

async fn github_response_error(resource: &str, response: reqwest::Response) -> String {
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.text().await.unwrap_or_default();
    format_github_response_error(resource, status, &headers, &body)
}

async fn github_response_error_limited(
    resource: &str,
    response: reqwest::Response,
    max: u64,
) -> String {
    let status = response.status();
    let headers = response.headers().clone();
    let body = match read_response_bytes_limited(response, max).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => error.to_string(),
    };
    format_github_response_error(resource, status, &headers, &body)
}

async fn read_response_bytes_limited(mut response: reqwest::Response, max: u64) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|length| length > max) {
        anyhow::bail!("response body exceeds download limit of {max} bytes");
    }

    let mut bytes = Vec::new();
    let mut downloaded = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .context("failed to read GitHub asset body")?
    {
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > max {
            anyhow::bail!("response body exceeds download limit of {max} bytes");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
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
    use std::{net::SocketAddr, time::Duration};

    use super::{ReleaseClient, format_github_response_error};
    use crate::repo::RepoRef;
    use reqwest::{
        StatusCode,
        header::{HeaderMap, HeaderValue},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::sleep,
    };

    async fn serve_one_response(
        status: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (SocketAddr, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let extra_headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
            body.len()
        );

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
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
            stream.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8(received).unwrap()
        });

        (addr, server)
    }

    #[tokio::test]
    async fn lists_release_page_and_detects_next_link() {
        let body = r#"[{
          "tag_name":"v2.0.0-beta.1",
          "draft":true,
          "prerelease":true,
          "assets":[]
        }]"#;
        let link = "<https://api.github.com/repos/owner/project/releases?page=2>; rel=\"next\", <https://api.github.com/repos/owner/project/releases?page=4>; rel=\"last\"";
        let (addr, server) = serve_one_response("200 OK", &[("Link", link)], body).await;
        let client = ReleaseClient::with_test_api_base_url(&format!("http://{addr}")).unwrap();
        let repo = RepoRef::parse("owner/project").unwrap();

        let page = client.releases_page(&repo, 2, 200).await.unwrap();
        let request = server.await.unwrap();

        assert_eq!(page.releases.len(), 1);
        assert!(page.releases[0].draft);
        assert!(page.releases[0].prerelease);
        assert!(page.has_next_page);
        assert!(
            request.starts_with("GET /repos/owner/project/releases?page=2&per_page=100 HTTP/1.1")
        );
    }

    #[tokio::test]
    async fn release_page_without_next_link_is_last_page() {
        let (addr, server) = serve_one_response("200 OK", &[], "[]").await;
        let client = ReleaseClient::with_test_api_base_url(&format!("http://{addr}")).unwrap();
        let repo = RepoRef::parse("owner/project").unwrap();

        let page = client.releases_page(&repo, 1, 25).await.unwrap();
        server.await.unwrap();

        assert!(!page.has_next_page);
    }

    #[tokio::test]
    async fn requests_release_tag_as_one_encoded_path_segment() {
        let body = r#"{"tag_name":"release/v1 beta","assets":[]}"#;
        let (addr, server) = serve_one_response("200 OK", &[], body).await;
        let client = ReleaseClient::with_test_api_base_url(&format!("http://{addr}")).unwrap();
        let repo = RepoRef::parse("owner/project").unwrap();

        let release = client
            .release_by_tag(&repo, "release/v1 beta")
            .await
            .unwrap()
            .unwrap();
        let request = server.await.unwrap();

        assert_eq!(release.tag_name, "release/v1 beta");
        assert!(
            request
                .starts_with("GET /repos/owner/project/releases/tags/release%2Fv1%20beta HTTP/1.1")
        );
    }

    #[tokio::test]
    async fn release_by_tag_returns_none_for_not_found() {
        let (addr, server) =
            serve_one_response("404 Not Found", &[], r#"{"message":"Not Found"}"#).await;
        let client = ReleaseClient::with_test_api_base_url(&format!("http://{addr}")).unwrap();
        let repo = RepoRef::parse("owner/project").unwrap();

        let release = client.release_by_tag(&repo, "missing").await.unwrap();
        server.await.unwrap();

        assert_eq!(release, None);
    }

    #[tokio::test]
    async fn checks_github_connectivity_without_requiring_a_repo() {
        let (addr, server) =
            serve_one_response("200 OK", &[], r#"{"resources":{"core":{"remaining":60}}}"#).await;
        let client = ReleaseClient::with_test_api_base_url(&format!("http://{addr}")).unwrap();

        client.check_connectivity().await.unwrap();
        let request = server.await.unwrap();

        assert!(request.starts_with("GET /rate_limit HTTP/1.1"));
    }

    #[tokio::test]
    async fn release_page_preserves_github_error_details() {
        let (addr, server) = serve_one_response(
            "403 Forbidden",
            &[("x-github-request-id", "request-123")],
            r#"{"message":"API rate limit exceeded"}"#,
        )
        .await;
        let client = ReleaseClient::with_test_api_base_url(&format!("http://{addr}")).unwrap();
        let repo = RepoRef::parse("owner/project").unwrap();

        let error = client.releases_page(&repo, 1, 100).await.unwrap_err();
        server.await.unwrap();

        let message = error.to_string();
        assert!(message.contains("GitHub releases request failed with 403 Forbidden"));
        assert!(message.contains("API rate limit exceeded"));
        assert!(message.contains("request id request-123"));
    }

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
    async fn limited_download_rejects_chunked_body_as_soon_as_limit_is_exceeded() {
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
                stream.write_all(b"5\r\nhello\r\n").await.unwrap();
                stream.write_all(b"6\r\n world\r\n").await.unwrap();
                stream.write_all(b"0\r\n\r\n").await.unwrap();
                stream.flush().await.unwrap();
            }
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let error = client
            .download_bytes_limited(&format!("http://{addr}/checksum"), 10)
            .await
            .unwrap_err();

        assert!(
            error.to_string().contains("exceeds download limit"),
            "{error:#}"
        );
        assert!(error.to_string().contains("10 bytes"), "{error:#}");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn limited_download_also_bounds_chunked_error_response_body() {
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
                        b"HTTP/1.1 400 Bad Request\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n\r\n",
                    )
                    .await
                    .unwrap();
                stream.write_all(b"B\r\nerror-body!\r\n").await.unwrap();
                stream.write_all(b"0\r\n\r\n").await.unwrap();
                stream.flush().await.unwrap();
            }
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let error = client
            .download_bytes_limited(&format!("http://{addr}/checksum"), 10)
            .await
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("400 Bad Request"), "{error:#}");
        assert!(
            message.contains("response body exceeds download limit of 10 bytes"),
            "{error:#}"
        );
        server.await.unwrap();
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
        assert!(error.to_string().contains("while downloading"), "{error:#}");
        let is_timeout = error.chain().any(|cause| {
            cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|request_error| request_error.is_timeout())
        });
        assert!(is_timeout, "{error:#}");

        server.abort();
    }
}
