use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::{Client, StatusCode, header};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::env;

type HmacSha256 = Hmac<Sha256>;

const GITHUB_API_URL: &str = "https://api.github.com";

#[derive(Clone, Debug)]
pub struct GithubAppConfig {
    pub app_id: String,
    pub app_slug: String,
    pub private_key: String,
    pub webhook_secret: String,
}

impl GithubAppConfig {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let app_id = optional_env("BELLA_GITHUB_APP_ID")?;
        let app_slug = optional_env("BELLA_GITHUB_APP_SLUG")?;
        let private_key = optional_env("BELLA_GITHUB_PRIVATE_KEY")?;
        let webhook_secret = optional_env("BELLA_GITHUB_WEBHOOK_SECRET")?;
        let values = [
            app_id.as_ref(),
            app_slug.as_ref(),
            private_key.as_ref(),
            webhook_secret.as_ref(),
        ];
        if values.iter().all(|value| value.is_none()) {
            return Ok(None);
        }
        let Some(app_id) = app_id else {
            anyhow::bail!("BELLA_GITHUB_APP_ID is required when GitHub integration is configured");
        };
        let Some(app_slug) = app_slug else {
            anyhow::bail!(
                "BELLA_GITHUB_APP_SLUG is required when GitHub integration is configured"
            );
        };
        let Some(private_key) = private_key else {
            anyhow::bail!(
                "BELLA_GITHUB_PRIVATE_KEY is required when GitHub integration is configured"
            );
        };
        let Some(webhook_secret) = webhook_secret else {
            anyhow::bail!(
                "BELLA_GITHUB_WEBHOOK_SECRET is required when GitHub integration is configured"
            );
        };
        Ok(Some(Self {
            app_id,
            app_slug,
            private_key: normalize_private_key(&private_key),
            webhook_secret,
        }))
    }

    pub fn install_url(&self, state: &str) -> String {
        format!(
            "https://github.com/apps/{}/installations/new?state={}",
            self.app_slug, state
        )
    }
}

#[derive(Clone)]
pub struct GithubClient {
    client: Client,
    config: GithubAppConfig,
}

impl GithubClient {
    pub fn new(client: Client, config: GithubAppConfig) -> Self {
        Self { client, config }
    }

    pub fn config(&self) -> &GithubAppConfig {
        &self.config
    }

    pub fn verify_webhook_signature(&self, signature: &str, body: &[u8]) -> bool {
        verify_webhook_signature(&self.config.webhook_secret, signature, body)
    }

    pub async fn installation(
        &self,
        installation_id: i64,
    ) -> Result<GithubInstallation, GithubError> {
        self.github_get(&format!("/app/installations/{installation_id}"), None)
            .await
    }

    pub async fn repositories(
        &self,
        installation_id: i64,
    ) -> Result<Vec<GithubRepository>, GithubError> {
        let token = self.installation_token(installation_id).await?;
        let mut repositories = Vec::new();
        let mut page = 1;
        loop {
            let response: GithubRepositoriesResponse = self
                .github_get(
                    &format!("/installation/repositories?per_page=100&page={page}"),
                    Some(&token.token),
                )
                .await?;
            let count = response.repositories.len();
            repositories.extend(response.repositories);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(repositories)
    }

    pub async fn create_pull_request(
        &self,
        installation_id: i64,
        request: &CreatePullRequest,
    ) -> Result<GithubPullRequest, GithubError> {
        let token = self.installation_token(installation_id).await?;
        self.github_post(
            &format!("/repos/{}/{}/pulls", request.owner, request.repo),
            Some(&token.token),
            &serde_json::json!({
                "title": request.title,
                "head": request.head,
                "base": request.base,
                "body": request.body,
                "draft": request.draft,
            }),
        )
        .await
    }

    pub async fn get_ref(
        &self,
        installation_id: i64,
        owner: &str,
        repo: &str,
        git_ref: &str,
    ) -> Result<GithubRef, GithubError> {
        let token = self.installation_token(installation_id).await?;
        self.github_get(
            &format!("/repos/{owner}/{repo}/git/ref/{git_ref}"),
            Some(&token.token),
        )
        .await
    }

    pub async fn create_ref(
        &self,
        installation_id: i64,
        request: &CreateRef,
    ) -> Result<GithubRef, GithubError> {
        let token = self.installation_token(installation_id).await?;
        self.github_post(
            &format!("/repos/{}/{}/git/refs", request.owner, request.repo),
            Some(&token.token),
            &serde_json::json!({
                "ref": request.git_ref,
                "sha": request.sha,
            }),
        )
        .await
    }

    pub async fn upsert_file(
        &self,
        installation_id: i64,
        request: &UpsertFile,
    ) -> Result<GithubContentCommit, GithubError> {
        let token = self.installation_token(installation_id).await?;
        let mut body = serde_json::json!({
            "message": request.message,
            "content": STANDARD.encode(request.content.as_bytes()),
            "branch": request.branch,
        });
        if let Some(sha) = &request.sha {
            body["sha"] = serde_json::Value::String(sha.clone());
        }
        self.github_put(
            &format!(
                "/repos/{}/{}/contents/{}",
                request.owner, request.repo, request.path
            ),
            Some(&token.token),
            &body,
        )
        .await
    }

    async fn installation_token(
        &self,
        installation_id: i64,
    ) -> Result<InstallationToken, GithubError> {
        self.github_post(
            &format!("/app/installations/{installation_id}/access_tokens"),
            None,
            &serde_json::json!({}),
        )
        .await
    }

    async fn github_get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        token: Option<&str>,
    ) -> Result<T, GithubError> {
        let jwt = token.is_none().then(|| self.app_jwt()).transpose()?;
        let mut request = self
            .client
            .get(format!("{GITHUB_API_URL}{path}"))
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(header::USER_AGENT, "bella");
        if let Some(token) = token {
            request = request.bearer_auth(token);
        } else if let Some(jwt) = jwt {
            request = request.bearer_auth(jwt);
        }
        self.send(request).await
    }

    async fn github_post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &serde_json::Value,
    ) -> Result<T, GithubError> {
        let jwt = token.is_none().then(|| self.app_jwt()).transpose()?;
        let mut request = self
            .client
            .post(format!("{GITHUB_API_URL}{path}"))
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(header::USER_AGENT, "bella")
            .json(body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        } else if let Some(jwt) = jwt {
            request = request.bearer_auth(jwt);
        }
        self.send(request).await
    }

    async fn github_put<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &serde_json::Value,
    ) -> Result<T, GithubError> {
        let jwt = token.is_none().then(|| self.app_jwt()).transpose()?;
        let mut request = self
            .client
            .put(format!("{GITHUB_API_URL}{path}"))
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(header::USER_AGENT, "bella")
            .json(body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        } else if let Some(jwt) = jwt {
            request = request.bearer_auth(jwt);
        }
        self.send(request).await
    }

    async fn send<T: for<'de> Deserialize<'de>>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, GithubError> {
        let response = request.send().await.map_err(|error| {
            tracing::warn!(%error, "GitHub request failed");
            GithubError::Unavailable
        })?;
        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(GithubError::Rejected);
        }
        let response = response.error_for_status().map_err(|error| {
            tracing::warn!(%error, "GitHub returned an error status");
            GithubError::Unavailable
        })?;
        response.json().await.map_err(|error| {
            tracing::warn!(%error, "GitHub returned an invalid response");
            GithubError::Unavailable
        })
    }

    fn app_jwt(&self) -> Result<String, GithubError> {
        #[derive(Serialize)]
        struct Claims<'a> {
            iat: i64,
            exp: i64,
            iss: &'a str,
        }

        let now = Utc::now();
        let claims = Claims {
            iat: (now - Duration::seconds(60)).timestamp(),
            exp: (now + Duration::minutes(9)).timestamp(),
            iss: &self.config.app_id,
        };
        let key =
            EncodingKey::from_rsa_pem(self.config.private_key.as_bytes()).map_err(|error| {
                tracing::warn!(%error, "invalid GitHub App private key");
                GithubError::Configuration
            })?;
        encode(&Header::new(Algorithm::RS256), &claims, &key).map_err(|error| {
            tracing::warn!(%error, "could not sign GitHub App JWT");
            GithubError::Configuration
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubInstallation {
    pub id: i64,
    pub account: GithubAccount,
    pub repository_selection: String,
    pub permissions: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubAccount {
    pub id: i64,
    pub login: String,
    #[serde(rename = "type")]
    pub account_type: String,
    pub html_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubRepository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub html_url: String,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct GithubRepositoriesResponse {
    repositories: Vec<GithubRepository>,
}

#[derive(Debug, Deserialize)]
struct InstallationToken {
    token: String,
}

#[derive(Debug, Serialize)]
pub struct CreatePullRequest {
    pub owner: String,
    pub repo: String,
    pub title: String,
    pub head: String,
    pub base: String,
    pub body: String,
    pub draft: bool,
}

#[derive(Debug, Serialize)]
pub struct CreateRef {
    pub owner: String,
    pub repo: String,
    pub git_ref: String,
    pub sha: String,
}

#[derive(Debug, Serialize)]
pub struct UpsertFile {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub path: String,
    pub message: String,
    pub content: String,
    pub sha: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GithubRef {
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub object: GithubRefObject,
}

#[derive(Debug, Deserialize)]
pub struct GithubRefObject {
    pub sha: String,
    #[serde(rename = "type")]
    pub object_type: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubContentCommit {
    pub commit: GithubCommitSummary,
    pub content: Option<GithubContentSummary>,
}

#[derive(Debug, Deserialize)]
pub struct GithubCommitSummary {
    pub sha: String,
    pub html_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubContentSummary {
    pub path: String,
    pub sha: String,
    pub html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GithubPullRequest {
    pub number: i64,
    pub html_url: String,
}

#[derive(Debug)]
pub enum GithubError {
    Configuration,
    Rejected,
    Unavailable,
}

pub fn verify_webhook_signature(secret: &str, signature: &str, body: &[u8]) -> bool {
    let Some(signature) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(signature) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn optional_env(name: &str) -> anyhow::Result<Option<String>> {
    match env::var(name) {
        Ok(value) => Ok(Some(value.trim().to_string()).filter(|value| !value.is_empty())),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn normalize_private_key(value: &str) -> String {
    value.replace("\\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::verify_webhook_signature;

    #[test]
    fn verifies_github_webhook_signature() {
        assert!(verify_webhook_signature(
            "secret",
            "sha256=b82fcb791acec57859b989b430a826488ce2e479fdf92326bd0a2e8375a42ba4",
            b"payload"
        ));
    }

    #[test]
    fn rejects_invalid_github_webhook_signature() {
        assert!(!verify_webhook_signature(
            "secret",
            "sha256=0000000000000000000000000000000000000000000000000000000000000000",
            b"payload"
        ));
    }
}
