use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RepoParseError {
    #[error("expected a GitHub repository in owner/repo form or a GitHub URL")]
    InvalidFormat,
    #[error("only GitHub repository URLs are supported in v1")]
    NonGitHubUrl,
}

impl RepoRef {
    pub fn parse(input: &str) -> Result<Self, RepoParseError> {
        let trimmed = input.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return Self::parse_url(trimmed);
        }

        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
            return Err(RepoParseError::InvalidFormat);
        }

        Ok(Self {
            owner: parts[0].to_string(),
            name: parts[1].trim_end_matches(".git").to_string(),
        })
    }

    pub fn id(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    pub fn github_url(&self) -> String {
        format!("https://github.com/{}", self.id())
    }

    fn parse_url(input: &str) -> Result<Self, RepoParseError> {
        let url = Url::parse(input).map_err(|_| RepoParseError::InvalidFormat)?;
        if url.host_str() != Some("github.com") {
            return Err(RepoParseError::NonGitHubUrl);
        }

        let mut segments = url
            .path_segments()
            .ok_or(RepoParseError::InvalidFormat)?
            .filter(|segment| !segment.is_empty());

        let owner = segments.next().ok_or(RepoParseError::InvalidFormat)?;
        let repo = segments.next().ok_or(RepoParseError::InvalidFormat)?;

        Ok(Self {
            owner: owner.to_string(),
            name: repo.trim_end_matches(".git").to_string(),
        })
    }
}
