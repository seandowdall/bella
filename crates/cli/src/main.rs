use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser)]
#[command(name = "bella")]
#[command(about = "Command-line client for Bella.")]
struct Cli {
    #[arg(
        long,
        env = "BELLA_API_BASE_URL",
        default_value = "http://127.0.0.1:3000"
    )]
    api_base_url: String,

    #[arg(long, global = true, help = "Print machine-readable JSON")]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the configured API base URL and authentication status.
    Config,
    /// Log in through GitHub in a browser.
    Login {
        #[arg(long, help = "Print the login URL instead of opening a browser")]
        no_open: bool,
    },
    /// Print the currently authenticated GitHub user.
    Whoami,
    /// Delete the locally stored CLI credential.
    Logout,
}

#[derive(Debug, Serialize, Deserialize)]
struct Credentials {
    api_base_url: String,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: String,
    github_login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Serialize)]
struct ConfigOutput<'a> {
    api_base_url: &'a str,
    authenticated: bool,
    credentials_path: String,
}

#[derive(Serialize)]
struct LoginStartRequest<'a> {
    poll_secret: &'a str,
}

#[derive(Deserialize)]
struct LoginStartResponse {
    request_id: String,
    verification_url: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Serialize)]
struct LoginPollRequest<'a> {
    request_id: &'a str,
    poll_secret: &'a str,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum LoginPollResponse {
    Pending,
    Complete { token: String, user: User },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let credentials_path = credentials_path()?;

    match cli.command {
        Command::Config => {
            let output = ConfigOutput {
                api_base_url: &cli.api_base_url,
                authenticated: load_credentials(&credentials_path)
                    .is_some_and(|credentials| credentials.api_base_url == cli.api_base_url),
                credentials_path: credentials_path.display().to_string(),
            };
            print_value(&output, cli.json, || {
                format!(
                    "api_base_url={}\nauthenticated={}\ncredentials_path={}",
                    output.api_base_url, output.authenticated, output.credentials_path
                )
            })?;
        }
        Command::Login { no_open } => {
            login(&cli.api_base_url, &credentials_path, no_open, cli.json).await?;
        }
        Command::Whoami => {
            let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
            let user = fetch_user(&cli.api_base_url, &credentials.token).await?;
            print_value(&user, cli.json, || {
                user.name
                    .as_ref()
                    .map(|name| format!("{name} (@{})", user.github_login))
                    .unwrap_or_else(|| format!("@{}", user.github_login))
            })?;
        }
        Command::Logout => {
            if let Some(credentials) = load_credentials(&credentials_path)
                && credentials.api_base_url == cli.api_base_url
            {
                revoke_token(&cli.api_base_url, &credentials.token).await?;
            }
            if credentials_path.exists() {
                fs::remove_file(&credentials_path)
                    .with_context(|| format!("failed to remove {}", credentials_path.display()))?;
            }
            if cli.json {
                println!("{}", serde_json::json!({ "logged_out": true }));
            } else {
                println!("Logged out.");
            }
        }
    }

    Ok(())
}

async fn login(
    api_base_url: &str,
    credentials_path: &Path,
    no_open: bool,
    json: bool,
) -> anyhow::Result<()> {
    let client = Client::new();
    let poll_secret = random_secret();
    let response = client
        .post(format!(
            "{}/v1/auth/cli/start",
            api_base_url.trim_end_matches('/')
        ))
        .json(&LoginStartRequest {
            poll_secret: &poll_secret,
        })
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the login request")?
        .json::<LoginStartResponse>()
        .await?;

    if no_open || webbrowser::open(&response.verification_url).is_err() {
        eprintln!(
            "Open this URL to authenticate:\n{}",
            response.verification_url
        );
    } else if !json {
        eprintln!("Opened GitHub login in your browser.");
    }
    if !json {
        eprintln!("Waiting for authentication...");
    }

    let attempts = response.expires_in / response.interval.max(1);
    for _ in 0..attempts {
        tokio::time::sleep(Duration::from_secs(response.interval)).await;
        let poll = client
            .post(format!(
                "{}/v1/auth/cli/poll",
                api_base_url.trim_end_matches('/')
            ))
            .json(&LoginPollRequest {
                request_id: &response.request_id,
                poll_secret: &poll_secret,
            })
            .send()
            .await
            .context("failed to poll Bella API")?;

        if poll.status() == StatusCode::BAD_REQUEST {
            bail!("login request expired or was already consumed");
        }
        let result = poll
            .error_for_status()
            .context("Bella API rejected the login poll")?
            .json::<LoginPollResponse>()
            .await?;
        if let LoginPollResponse::Complete { token, user } = result {
            save_credentials(
                credentials_path,
                &Credentials {
                    api_base_url: api_base_url.to_owned(),
                    token,
                },
            )?;
            print_value(&user, json, || {
                format!("Logged in as @{}.", user.github_login)
            })?;
            return Ok(());
        }
    }

    bail!("login timed out");
}

async fn fetch_user(api_base_url: &str, token: &str) -> anyhow::Result<User> {
    Client::new()
        .get(format!("{}/v1/me", api_base_url.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("stored credential is invalid; run `bella login` again")?
        .json()
        .await
        .context("Bella API returned an invalid user response")
}

async fn revoke_token(api_base_url: &str, token: &str) -> anyhow::Result<()> {
    Client::new()
        .post(format!(
            "{}/v1/auth/token/revoke",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to contact Bella API; credential was not removed")?
        .error_for_status()
        .context("Bella API rejected token revocation")?;
    Ok(())
}

fn credentials_path() -> anyhow::Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("bella")
        .join("credentials.json"))
}

fn save_credentials(path: &Path, credentials: &Credentials) -> anyhow::Result<()> {
    let parent = path.parent().context("invalid credentials path")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let data = serde_json::to_vec_pretty(credentials)?;
    fs::write(path, data).with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn load_credentials(path: &Path) -> Option<Credentials> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn require_credentials(path: &Path, api_base_url: &str) -> anyhow::Result<Credentials> {
    let credentials = load_credentials(path).context("not logged in; run `bella login` first")?;
    if credentials.api_base_url != api_base_url {
        bail!(
            "credential is for {}; run `bella login` for {}",
            credentials.api_base_url,
            api_base_url
        );
    }
    Ok(credentials)
}

fn random_secret() -> String {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rand::{RngCore, rngs::OsRng};

    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn print_value<T, F>(value: &T, json: bool, human: F) -> anyhow::Result<()>
where
    T: Serialize,
    F: FnOnce() -> String,
{
    if json {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", human());
    }
    Ok(())
}
