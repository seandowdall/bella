use chrono::{Duration, SecondsFormat, Utc};
use reqwest::{Client, StatusCode};

#[derive(Debug)]
pub struct ValidationResult {
    pub status: &'static str,
    pub validated_at: Option<chrono::DateTime<Utc>>,
    pub error: Option<String>,
}

impl ValidationResult {
    fn verified() -> Self {
        Self {
            status: "verified",
            validated_at: Some(Utc::now()),
            error: None,
        }
    }

    fn failed(status: &'static str, error: impl Into<String>) -> Self {
        Self {
            status,
            validated_at: Some(Utc::now()),
            error: Some(error.into()),
        }
    }

    fn unavailable(error: impl Into<String>) -> Self {
        Self {
            status: "validation_unavailable",
            validated_at: None,
            error: Some(error.into()),
        }
    }

    fn unsupported() -> Self {
        Self {
            status: "saved_unverified",
            validated_at: None,
            error: Some("Automatic validation is not available for this provider yet.".to_owned()),
        }
    }
}

pub async fn validate(
    client: &Client,
    provider: &str,
    secret: &str,
    openai_base_url: &str,
) -> ValidationResult {
    let request = match provider {
        "openai" => {
            let start_time = (Utc::now() - Duration::hours(1)).timestamp();
            let url = format!(
                "{}/v1/organization/costs",
                openai_base_url.trim_end_matches('/')
            );
            client
                .get(url)
                .bearer_auth(secret)
                .query(&[("start_time", start_time), ("limit", 1_i64)])
        }
        "anthropic" => {
            let starting_at =
                (Utc::now() - Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Secs, true);
            client
                .get("https://api.anthropic.com/v1/organizations/usage_report/messages")
                .header("x-api-key", secret)
                .header("anthropic-version", "2023-06-01")
                .query(&[
                    ("starting_at", starting_at.as_str()),
                    ("bucket_width", "1h"),
                    ("limit", "1"),
                ])
        }
        "mistral" => client
            .get("https://api.mistral.ai/v1/models")
            .bearer_auth(secret),
        "deepseek" => client
            .get("https://api.deepseek.com/models")
            .bearer_auth(secret),
        _ => return ValidationResult::unsupported(),
    };

    match request.send().await {
        Ok(response) if response.status().is_success() => ValidationResult::verified(),
        Ok(response) => from_status(response.status()),
        Err(error) => {
            tracing::warn!(%error, %provider, "provider credential validation unavailable");
            ValidationResult::unavailable("Could not reach the provider validation endpoint.")
        }
    }
}

fn from_status(status: StatusCode) -> ValidationResult {
    match status {
        StatusCode::UNAUTHORIZED => ValidationResult::failed(
            "invalid_credentials",
            "The provider rejected this credential.",
        ),
        StatusCode::FORBIDDEN => ValidationResult::failed(
            "insufficient_permissions",
            "The credential is valid but lacks the required permissions.",
        ),
        StatusCode::TOO_MANY_REQUESTS => ValidationResult::unavailable(
            "The provider rate-limited validation. Reconnect to try again later.",
        ),
        status if status.is_server_error() => {
            ValidationResult::unavailable("The provider validation service is unavailable.")
        }
        status => ValidationResult::unavailable(format!(
            "The provider returned HTTP {} during validation.",
            status.as_u16()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::from_status;
    use reqwest::StatusCode;

    #[test]
    fn classifies_provider_validation_responses() {
        assert_eq!(
            from_status(StatusCode::UNAUTHORIZED).status,
            "invalid_credentials"
        );
        assert_eq!(
            from_status(StatusCode::FORBIDDEN).status,
            "insufficient_permissions"
        );
        assert_eq!(
            from_status(StatusCode::TOO_MANY_REQUESTS).status,
            "validation_unavailable"
        );
    }
}
