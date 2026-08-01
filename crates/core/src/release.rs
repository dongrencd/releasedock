use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{
    Proxy, StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, LINK, RANGE, RETRY_AFTER, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::repo::RepoRef;

const GITHUB_API_TIMEOUT: Duration = Duration::from_secs(20);
const GITHUB_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const GITHUB_DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(60);
const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const DOWNLOAD_MAX_ATTEMPTS: usize = 3;
const DEFAULT_ACCELERATED_DOWNLOAD_CONNECTIONS: u8 = 4;
const MIN_ACCELERATED_DOWNLOAD_SIZE: u64 = 32 * 1024 * 1024;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySearchResult {
    pub repo_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub html_url: String,
    pub stars: u64,
}

#[derive(Debug, Deserialize)]
struct RepositorySearchResponse {
    #[serde(default)]
    items: Vec<GitHubRepositorySearchItem>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositorySearchItem {
    full_name: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    html_url: String,
    #[serde(default)]
    stargazers_count: u64,
}

impl From<GitHubRepositorySearchItem> for RepositorySearchResult {
    fn from(item: GitHubRepositorySearchItem) -> Self {
        Self {
            repo_id: item.full_name,
            name: item.name,
            description: item.description,
            html_url: item.html_url,
            stars: item.stargazers_count,
        }
    }
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
    download_acceleration: DownloadAcceleration,
}

#[derive(Debug, Clone, Copy)]
struct DownloadAcceleration {
    enabled: bool,
    max_connections: u8,
}

impl Default for DownloadAcceleration {
    fn default() -> Self {
        Self {
            enabled: true,
            max_connections: DEFAULT_ACCELERATED_DOWNLOAD_CONNECTIONS,
        }
    }
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
            download_acceleration: DownloadAcceleration::default(),
        })
    }

    pub fn with_download_acceleration(mut self, enabled: bool, max_connections: u8) -> Self {
        self.download_acceleration = DownloadAcceleration {
            enabled,
            max_connections: max_connections.clamp(1, 8),
        };
        self
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

    pub async fn search_repositories(
        &self,
        query: &str,
        per_page: u32,
    ) -> Result<Vec<RepositorySearchResult>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let mut url = self.api_base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("GitHub API base URL cannot contain path segments"))?;
            segments.pop_if_empty();
            segments.extend(["search", "repositories"]);
        }
        url.query_pairs_mut()
            .append_pair("q", trimmed)
            .append_pair("per_page", &per_page.clamp(1, 10).to_string());

        let response = self
            .client
            .get(url)
            .timeout(self.api_timeout)
            .send()
            .await
            .context("failed to request GitHub repository search")?;

        if !response.status().is_success() {
            let error = github_response_error("GitHub repository search", response).await;
            anyhow::bail!(error);
        }

        let response = response
            .json::<RepositorySearchResponse>()
            .await
            .context("failed to parse GitHub repository search response")?;
        Ok(response.items.into_iter().map(Into::into).collect())
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
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create download directory {}", parent.display())
            })?;
        }

        let part_path = partial_download_path(path);
        let mut last_error = None;
        for attempt in 1..=DOWNLOAD_MAX_ATTEMPTS {
            match self
                .download_to_part(url, path, &part_path, &mut on_progress)
                .await
            {
                Ok(()) => return Ok(()),
                Err(error)
                    if attempt < DOWNLOAD_MAX_ATTEMPTS && is_temporary_download_error(&error) =>
                {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("download failed after retries")))
    }

    async fn download_to_part<F>(
        &self,
        url: &str,
        path: &Path,
        part_path: &Path,
        on_progress: &mut F,
    ) -> Result<()>
    where
        F: FnMut(u64, Option<u64>),
    {
        let resume_from = fs::metadata(part_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if resume_from == 0 {
            match self.probe_accelerated_download(url).await {
                Ok(Some(total)) => {
                    match self
                        .download_to_segments(url, path, part_path, total, on_progress)
                        .await
                    {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            if !is_temporary_download_error(&error) {
                                return Err(error);
                            }
                            eprintln!(
                                "accelerated download failed for {url}; falling back to single connection: {error:#}"
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    if !is_temporary_download_error(&error) {
                        return Err(error);
                    }
                    eprintln!(
                        "accelerated download probe failed for {url}; falling back to single connection: {error:#}"
                    );
                }
            }
        }
        let mut request = self.client.get(url);
        if resume_from > 0 {
            request = request.header(RANGE, format!("bytes={resume_from}-"));
        }

        let mut response = request
            .send()
            .await
            .context("failed to request GitHub asset")?;

        if !response.status().is_success() {
            let error = github_response_error("GitHub asset", response).await;
            anyhow::bail!(error);
        }

        let (mut file, mut downloaded, total) = prepare_download_file(
            part_path,
            response.status(),
            resume_from,
            response.headers(),
        )?;
        on_progress(downloaded, total);

        while let Some(chunk) = response
            .chunk()
            .await
            .context("failed to read GitHub asset body while downloading")?
        {
            file.write_all(&chunk)
                .with_context(|| format!("failed to write download {}", part_path.display()))?;
            downloaded += chunk.len() as u64;
            on_progress(downloaded, total);
        }

        file.flush()
            .with_context(|| format!("failed to flush download {}", part_path.display()))?;
        replace_download(path, part_path)?;
        Ok(())
    }

    async fn probe_accelerated_download(&self, url: &str) -> Result<Option<u64>> {
        if !self.download_acceleration.enabled || self.download_acceleration.max_connections <= 1 {
            return Ok(None);
        }

        let response = self
            .client
            .get(url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .context("failed to probe GitHub asset range support")?;
        if !response.status().is_success() {
            let error = github_response_error("GitHub asset", response).await;
            anyhow::bail!(error);
        }
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Ok(None);
        }
        Ok(content_range_total(response.headers())
            .filter(|total| *total >= MIN_ACCELERATED_DOWNLOAD_SIZE))
    }

    async fn download_to_segments<F>(
        &self,
        url: &str,
        path: &Path,
        part_path: &Path,
        total: u64,
        on_progress: &mut F,
    ) -> Result<()>
    where
        F: FnMut(u64, Option<u64>),
    {
        let connections = self
            .download_acceleration
            .max_connections
            .min(total.min(u64::from(u8::MAX)).max(1) as u8)
            .max(1);
        let segments = download_segments(total, connections);
        let already_downloaded = existing_segment_bytes(part_path, &segments);
        on_progress(already_downloaded, Some(total));

        let mut handles = Vec::new();
        for segment in segments.clone() {
            let segment_path = segment_download_path(part_path, segment.index);
            if fs::metadata(&segment_path)
                .map(|metadata| metadata.len() == segment.len())
                .unwrap_or(false)
            {
                continue;
            }
            let client = self.client.clone();
            let url = url.to_string();
            handles.push(tokio::spawn(async move {
                download_segment(client, url, segment, segment_path).await
            }));
        }

        let mut downloaded = already_downloaded;
        for handle in handles {
            let segment_bytes = handle.await.context("download segment task failed")??;
            downloaded = downloaded.saturating_add(segment_bytes);
            on_progress(downloaded.min(total), Some(total));
        }

        merge_download_segments(part_path, &segments)?;
        replace_download(path, part_path)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct DownloadSegment {
    index: usize,
    start: u64,
    end: u64,
}

impl DownloadSegment {
    fn len(&self) -> u64 {
        self.end - self.start + 1
    }
}

fn download_segments(total: u64, connections: u8) -> Vec<DownloadSegment> {
    let connections = u64::from(connections).max(1).min(total.max(1));
    let base = total / connections;
    let remainder = total % connections;
    let mut start = 0u64;
    let mut segments = Vec::new();

    for index in 0..connections {
        let len = base + u64::from(index < remainder);
        let end = start + len - 1;
        segments.push(DownloadSegment {
            index: index as usize,
            start,
            end,
        });
        start = end + 1;
    }
    segments
}

async fn download_segment(
    client: reqwest::Client,
    url: String,
    segment: DownloadSegment,
    segment_path: PathBuf,
) -> Result<u64> {
    let existing = fs::metadata(&segment_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing > segment.len() {
        fs::remove_file(&segment_path)
            .with_context(|| format!("failed to reset segment {}", segment_path.display()))?;
    }
    let existing = fs::metadata(&segment_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing == segment.len() {
        return Ok(0);
    }

    let start = segment.start + existing;
    let request_range = format!("bytes={start}-{}", segment.end);
    let mut response = client
        .get(url)
        .header(RANGE, request_range)
        .send()
        .await
        .context("failed to request GitHub asset segment")?;
    if response.status() != StatusCode::PARTIAL_CONTENT {
        let error = github_response_error("GitHub asset", response).await;
        anyhow::bail!("accelerated segment download expected 206 Partial Content: {error}");
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&segment_path)
        .with_context(|| format!("failed to open segment {}", segment_path.display()))?;
    let mut written = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .context("failed to read GitHub asset segment body")?
    {
        file.write_all(&chunk)
            .with_context(|| format!("failed to write segment {}", segment_path.display()))?;
        written += chunk.len() as u64;
    }
    file.flush()
        .with_context(|| format!("failed to flush segment {}", segment_path.display()))?;
    let final_len = fs::metadata(&segment_path)
        .map(|metadata| metadata.len())
        .with_context(|| format!("failed to inspect segment {}", segment_path.display()))?;
    if final_len != segment.len() {
        anyhow::bail!(
            "segment {} has {} bytes, expected {}",
            segment.index,
            final_len,
            segment.len()
        );
    }
    Ok(written)
}

fn prepare_download_file(
    part_path: &Path,
    status: StatusCode,
    resume_from: u64,
    headers: &HeaderMap,
) -> Result<(fs::File, u64, Option<u64>)> {
    if resume_from > 0 && status == StatusCode::PARTIAL_CONTENT {
        let total = content_range_total(headers).or_else(|| {
            headers
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|remaining| resume_from + remaining)
        });
        let file = fs::OpenOptions::new()
            .append(true)
            .open(part_path)
            .with_context(|| format!("failed to open partial download {}", part_path.display()))?;
        return Ok((file, resume_from, total));
    }

    // Some CDNs ignore Range and return a complete 200 OK response. In that
    // case the stale partial file must be replaced instead of appended.
    let file = fs::File::create(part_path)
        .with_context(|| format!("failed to create download {}", part_path.display()))?;
    Ok((
        file,
        0,
        headers
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok()),
    ))
}

fn partial_download_path(path: &Path) -> std::path::PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.part"))
        .unwrap_or_else(|| "download.part".to_string());
    path.with_file_name(file_name)
}

fn segment_download_path(part_path: &Path, index: usize) -> PathBuf {
    let file_name = part_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.{index}"))
        .unwrap_or_else(|| format!("download.part.{index}"));
    part_path.with_file_name(file_name)
}

fn existing_segment_bytes(part_path: &Path, segments: &[DownloadSegment]) -> u64 {
    segments
        .iter()
        .map(|segment| {
            fs::metadata(segment_download_path(part_path, segment.index))
                .map(|metadata| metadata.len().min(segment.len()))
                .unwrap_or(0)
        })
        .sum()
}

fn merge_download_segments(part_path: &Path, segments: &[DownloadSegment]) -> Result<()> {
    let mut output = fs::File::create(part_path)
        .with_context(|| format!("failed to create download {}", part_path.display()))?;
    let mut buffer = [0u8; 64 * 1024];

    for segment in segments {
        let segment_path = segment_download_path(part_path, segment.index);
        let mut input = fs::File::open(&segment_path)
            .with_context(|| format!("failed to open segment {}", segment_path.display()))?;
        let mut copied = 0u64;
        loop {
            let read = input
                .read(&mut buffer)
                .with_context(|| format!("failed to read segment {}", segment_path.display()))?;
            if read == 0 {
                break;
            }
            copied += read as u64;
            output
                .write_all(&buffer[..read])
                .with_context(|| format!("failed to merge segment {}", segment_path.display()))?;
        }
        if copied != segment.len() {
            anyhow::bail!(
                "segment {} has {} bytes, expected {}",
                segment.index,
                copied,
                segment.len()
            );
        }
    }
    output
        .flush()
        .with_context(|| format!("failed to flush download {}", part_path.display()))?;

    for segment in segments {
        let segment_path = segment_download_path(part_path, segment.index);
        if let Err(error) = fs::remove_file(&segment_path)
            && segment_path.exists()
        {
            return Err(error)
                .with_context(|| format!("failed to remove segment {}", segment_path.display()));
        }
    }
    Ok(())
}

fn replace_download(path: &Path, part_path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("failed to replace old download {}", path.display()))?;
    }
    fs::rename(part_path, path).with_context(|| {
        format!(
            "failed to finalize download {} from {}",
            path.display(),
            part_path.display()
        )
    })
}

fn content_range_total(headers: &HeaderMap) -> Option<u64> {
    let value = headers.get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let (_, total) = value.rsplit_once('/')?;
    total.parse::<u64>().ok()
}

fn is_temporary_download_error(error: &anyhow::Error) -> bool {
    if error.to_string().contains(" 5") {
        return true;
    }
    error.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(|request_error| {
                request_error.is_timeout()
                    || request_error.is_connect()
                    || request_error.is_request()
                    || request_error.is_body()
            })
    })
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
    use std::{
        net::SocketAddr,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::{
        DOWNLOAD_MAX_ATTEMPTS, ReleaseClient, download_segments, format_github_response_error,
        segment_download_path,
    };
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

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
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
        String::from_utf8(received).unwrap()
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
    async fn searches_repositories_with_bounded_page_size() {
        let body = r#"{
          "items": [
            {
              "full_name": "owner/project",
              "name": "project",
              "description": "Project releases",
              "html_url": "https://github.com/owner/project",
              "stargazers_count": 42
            }
          ]
        }"#;
        let (addr, server) = serve_one_response("200 OK", &[], body).await;
        let client = ReleaseClient::with_test_api_base_url(&format!("http://{addr}")).unwrap();

        let results = client
            .search_repositories("release dock", 25)
            .await
            .unwrap();
        let request = server.await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].repo_id, "owner/project");
        assert_eq!(results[0].name, "project");
        assert_eq!(results[0].description.as_deref(), Some("Project releases"));
        assert_eq!(results[0].html_url, "https://github.com/owner/project");
        assert_eq!(results[0].stars, 42);
        assert!(
            request.starts_with("GET /search/repositories?q=release+dock&per_page=10 HTTP/1.1")
        );
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
        .unwrap()
        .with_download_acceleration(false, 1);
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
    async fn resumes_download_from_existing_part_file() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            stream
                .write_all(
                    b"HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 6-10/11\r\nAccept-Ranges: bytes\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\nworld",
                )
                .await
                .unwrap();
            request
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");
        let part_path = download_dir.path().join("asset.bin.part");
        std::fs::write(&part_path, b"hello ").unwrap();

        client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap();
        let request = server.await.unwrap();

        assert!(request.contains("range: bytes=6-"), "{request}");
        assert_eq!(std::fs::read(&download_path).unwrap(), b"hello world");
        assert!(!part_path.exists());
    }

    #[tokio::test]
    async fn restarts_download_when_server_ignores_resume_range() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\nfresh",
                )
                .await
                .unwrap();
            request
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");
        let part_path = download_dir.path().join("asset.bin.part");
        std::fs::write(&part_path, b"stale-prefix").unwrap();

        client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap();
        let request = server.await.unwrap();

        assert!(request.contains("range: bytes=12-"), "{request}");
        assert_eq!(std::fs::read(&download_path).unwrap(), b"fresh");
        assert!(!part_path.exists());
    }

    #[tokio::test]
    async fn falls_back_to_single_connection_when_range_probe_fails_transiently() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);

        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let _request = read_http_request(&mut stream).await;
                let current = server_attempts.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    continue;
                }
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\npayload",
                    )
                    .await
                    .unwrap();
            }
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");

        client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap();

        server.await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(std::fs::read(&download_path).unwrap(), b"payload");
    }

    #[tokio::test]
    async fn accelerates_large_range_downloads_with_multiple_segments() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let payload = Arc::new(
            (0..(33 * 1024 * 1024))
                .map(|index| (index % 251) as u8)
                .collect::<Vec<_>>(),
        );
        let requested_ranges = Arc::new(Mutex::new(Vec::new()));
        let server_payload = Arc::clone(&payload);
        let server_ranges = Arc::clone(&requested_ranges);

        let server = tokio::spawn(async move {
            let mut served_segments = 0usize;
            while served_segments < 4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                if request.contains("range: bytes=0-0") {
                    stream
                        .write_all(
                            format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                                server_payload.len()
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                    stream.write_all(&server_payload[0..1]).await.unwrap();
                    continue;
                }

                let Some(range) = request
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .and_then(|value| value.split_once('-'))
                    .and_then(|(start, end)| {
                        Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                    })
                else {
                    panic!("expected segmented range request, got {request}");
                };
                server_ranges.lock().unwrap().push(range);
                let (start, end) = range;
                let body = &server_payload[start..=end];
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            body.len(),
                            server_payload.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                stream.write_all(body).await.unwrap();
                served_segments += 1;
            }
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");

        client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap();

        server.await.unwrap();
        assert_eq!(std::fs::read(&download_path).unwrap(), payload.as_slice());
        assert!(
            requested_ranges.lock().unwrap().len() > 1,
            "download should split the file into multiple range requests"
        );
    }

    #[tokio::test]
    async fn resumes_accelerated_download_segments_without_refetching_complete_segments() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let payload = Arc::new(
            (0..(33 * 1024 * 1024))
                .map(|index| (index % 251) as u8)
                .collect::<Vec<_>>(),
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server_payload = Arc::clone(&payload);
        let server_requests = Arc::clone(&requests);

        let server = tokio::spawn(async move {
            let mut served_segments = 0usize;
            while served_segments < 3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                if request.contains("range: bytes=0-0") {
                    stream
                        .write_all(
                            format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                                server_payload.len()
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                    stream.write_all(&server_payload[0..1]).await.unwrap();
                    continue;
                }
                let Some(range) = request
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .and_then(|value| value.split_once('-'))
                    .and_then(|(start, end)| {
                        Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                    })
                else {
                    panic!("expected segmented range request, got {request}");
                };
                server_requests.lock().unwrap().push(range);
                let (start, end) = range;
                let body = &server_payload[start..=end];
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            body.len(),
                            server_payload.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                stream.write_all(body).await.unwrap();
                served_segments += 1;
            }
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");
        let part_path = download_dir.path().join("asset.bin.part");
        let segments = download_segments(payload.len() as u64, 4);
        std::fs::write(
            segment_download_path(&part_path, 0),
            &payload[segments[0].start as usize..=segments[0].end as usize],
        )
        .unwrap();
        std::fs::write(
            segment_download_path(&part_path, 1),
            &payload[segments[1].start as usize..(segments[1].start as usize + 128)],
        )
        .unwrap();

        client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap();

        server.await.unwrap();
        assert_eq!(std::fs::read(&download_path).unwrap(), payload.as_slice());
        let ranges = requests.lock().unwrap();
        assert!(!ranges.contains(&(segments[0].start as usize, segments[0].end as usize)));
        assert!(ranges.contains(&(segments[1].start as usize + 128, segments[1].end as usize)));
    }

    #[tokio::test]
    async fn retries_temporary_download_failures_and_keeps_final_file() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);

        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let _request = read_http_request(&mut stream).await;
                let current = server_attempts.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    continue;
                }
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\npayload",
                    )
                    .await
                    .unwrap();
            }
        });

        let client = ReleaseClient::new(None, None)
            .unwrap()
            .with_download_acceleration(false, 1);
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");

        client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap();

        server.await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(std::fs::read(&download_path).unwrap(), b"payload");
    }

    #[tokio::test]
    async fn does_not_retry_permanent_asset_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _request = read_http_request(&mut stream).await;
            server_attempts.fetch_add(1, Ordering::SeqCst);
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 24\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"message\":\"not found\"}",
                )
                .await
                .unwrap();
        });

        let client = ReleaseClient::new(None, None).unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");

        let error = client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap_err();

        server.await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(error.to_string().contains("404 Not Found"), "{error:#}");
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
            for _ in 0..DOWNLOAD_MAX_ATTEMPTS {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let _request = read_http_request(&mut stream).await;

                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/octet-stream\r\n\r\n",
                        )
                        .await;
                    let _ = stream.flush().await;

                    let _ = stream.write_all(b"5\r\nhello\r\n").await;
                    let _ = stream.flush().await;
                    sleep(Duration::from_millis(300)).await;
                }
            }
        });

        let client = ReleaseClient::with_timeouts(
            None,
            None,
            Duration::from_millis(500),
            Duration::from_millis(100),
        )
        .unwrap()
        .with_download_acceleration(false, 1);
        let download_dir = tempfile::tempdir().unwrap();
        let download_path = download_dir.path().join("asset.bin");

        let error = client
            .download_to_path(&format!("http://{addr}/asset"), &download_path, |_, _| {})
            .await
            .unwrap_err();
        let is_timeout = error.chain().any(|cause| {
            cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|request_error| request_error.is_timeout())
        });
        assert!(is_timeout, "{error:#}");

        server.abort();
    }
}
