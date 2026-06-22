use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Read},
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
    /// List provider types supported by this Bella server.
    #[command(hide = true)]
    Catalog,
    /// Log in through GitHub in a browser.
    Login {
        #[arg(long, help = "Print the login URL instead of opening a browser")]
        no_open: bool,
    },
    /// Print the currently authenticated GitHub user.
    Whoami,
    /// Delete the locally stored CLI credential.
    Logout,
    /// Manage organizations.
    Organizations {
        #[command(subcommand)]
        command: OrganizationCommand,
    },
    /// Manage AI provider connections.
    Providers {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Manage operational integrations.
    Integrations {
        #[command(subcommand)]
        command: IntegrationCommand,
    },
}

#[derive(Debug, Subcommand)]
enum OrganizationCommand {
    /// List organizations available to the current user.
    List,
    /// Create an organization with a default workspace.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, help = "Stable retry key; generated automatically when omitted")]
        idempotency_key: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ProviderCommand {
    /// List provider types supported by this Bella server.
    Catalog,
    /// List connected provider accounts for an organization.
    #[command(visible_alias = "accounts")]
    List {
        #[arg(long)]
        organization: Option<String>,
    },
    /// Connect or rotate a provider account credential.
    Connect {
        #[arg(long)]
        organization: String,
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        provider: String,
        #[arg(long)]
        name: String,
        #[arg(
            long,
            env = "BELLA_PROVIDER_SECRET",
            hide_env_values = true,
            help = "Provider credential; prefer BELLA_PROVIDER_SECRET or --secret-stdin"
        )]
        secret: Option<String>,
        #[arg(long, help = "Read the provider credential from standard input")]
        secret_stdin: bool,
    },
    /// Disconnect a provider account and delete its encrypted credential.
    Disconnect {
        #[arg(long)]
        organization: String,
        #[arg(long)]
        account: String,
    },
    /// Run an immediate sync for a provider account.
    Sync {
        #[arg(long)]
        organization: String,
        #[arg(long)]
        account: String,
    },
}

#[derive(Debug, Subcommand)]
enum IntegrationCommand {
    /// List connected integrations for an organization.
    List {
        #[arg(long)]
        organization: Option<String>,
    },
    /// Connect or rotate a PostHog webhook secret.
    Posthog {
        #[command(subcommand)]
        command: PosthogCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PosthogCommand {
    /// Generate or rotate the PostHog webhook secret.
    Connect {
        #[arg(long)]
        organization: Option<String>,
        #[arg(long, default_value = "PostHog")]
        name: String,
        #[arg(
            long,
            env = "BELLA_PUBLIC_API_URL",
            help = "Public Bella API origin used to print the PostHog webhook URL"
        )]
        public_api_url: Option<String>,
    },
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

#[derive(Debug, Serialize, Deserialize)]
struct Organization {
    id: String,
    slug: String,
    name: String,
    role: String,
    default_workspace: Workspace,
}

#[derive(Debug, Serialize, Deserialize)]
struct Workspace {
    id: String,
    slug: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProviderDefinition {
    id: String,
    name: String,
    category: String,
    ingestion: String,
    credential_label: String,
    credential_placeholder: String,
    docs_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProviderAccount {
    id: String,
    organization_id: String,
    workspace_id: String,
    workspace_name: String,
    provider: String,
    display_name: String,
    credential_fingerprint: String,
    status: String,
    validated_at: Option<String>,
    validation_error: Option<String>,
    last_synced_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SyncOutcome {
    sync_run_id: String,
    provider_account_id: String,
    provider: String,
    window_start: String,
    window_end: String,
    usage_buckets: usize,
    cost_snapshots: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct Integration {
    id: String,
    integration_type: String,
    display_name: String,
    status: String,
    credential_fingerprint: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PosthogConnection {
    integration: Integration,
    webhook_secret: String,
}

#[derive(Debug, Serialize)]
struct PosthogConnectionOutput {
    integration: Integration,
    webhook_url: String,
    webhook_secret: String,
    auth_header: String,
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

#[derive(Serialize)]
struct CreateOrganizationRequest<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct ConnectProviderRequest<'a> {
    workspace_id: &'a str,
    provider: &'a str,
    display_name: &'a str,
    credentials: BTreeMap<&'static str, &'a str>,
}

#[derive(Serialize)]
struct ConnectPosthogRequest<'a> {
    display_name: &'a str,
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
        Command::Catalog => {
            eprintln!(
                "Note: `bella catalog` lists supported provider types. \
                 Use `bella providers accounts` to list connected accounts."
            );
            print_provider_catalog(&cli.api_base_url, cli.json).await?;
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
        Command::Organizations { command } => {
            let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
            match command {
                OrganizationCommand::List => {
                    let organizations =
                        list_organizations(&cli.api_base_url, &credentials.token).await?;
                    print_value(&organizations, cli.json, || {
                        organizations
                            .iter()
                            .map(|organization| {
                                format!(
                                    "{}\t{}\t{}\t{}",
                                    organization.slug,
                                    organization.name,
                                    organization.role,
                                    organization.default_workspace.slug
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })?;
                }
                OrganizationCommand::Create {
                    name,
                    idempotency_key,
                } => {
                    let idempotency_key = idempotency_key.unwrap_or_else(random_secret);
                    let organization = create_organization(
                        &cli.api_base_url,
                        &credentials.token,
                        &name,
                        &idempotency_key,
                    )
                    .await?;
                    print_value(&organization, cli.json, || {
                        format!(
                            "Created {} ({}) with workspace {}.",
                            organization.name,
                            organization.slug,
                            organization.default_workspace.slug
                        )
                    })?;
                }
            }
        }
        Command::Providers { command } => match command {
            ProviderCommand::Catalog => {
                print_provider_catalog(&cli.api_base_url, cli.json).await?;
            }
            ProviderCommand::List { organization } => {
                let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
                let organization =
                    resolve_organization_id(&cli.api_base_url, &credentials.token, organization)
                        .await?;
                let accounts =
                    list_provider_accounts(&cli.api_base_url, &credentials.token, &organization)
                        .await?;
                print_value(&accounts, cli.json, || {
                    accounts
                        .iter()
                        .map(|account| {
                            format!(
                                "{}\t{}\t{}\t{}\t{}",
                                account.id,
                                account.provider,
                                account.display_name,
                                account.workspace_name,
                                account.status
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })?;
            }
            ProviderCommand::Connect {
                organization,
                workspace,
                provider,
                name,
                secret,
                secret_stdin,
            } => {
                let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
                let secret = read_provider_secret(secret, secret_stdin)?;
                let account = connect_provider(
                    &cli.api_base_url,
                    &credentials.token,
                    &organization,
                    &workspace,
                    &provider,
                    &name,
                    &secret,
                )
                .await?;
                print_value(&account, cli.json, || {
                    format!(
                        "Connected {} account {} ({}) in workspace {}.",
                        account.provider, account.display_name, account.id, account.workspace_name
                    )
                })?;
            }
            ProviderCommand::Disconnect {
                organization,
                account,
            } => {
                let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
                disconnect_provider(
                    &cli.api_base_url,
                    &credentials.token,
                    &organization,
                    &account,
                )
                .await?;
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({ "disconnected": true, "account_id": account })
                    );
                } else {
                    println!("Disconnected provider account {account}.");
                }
            }
            ProviderCommand::Sync {
                organization,
                account,
            } => {
                let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
                let outcome = sync_provider(
                    &cli.api_base_url,
                    &credentials.token,
                    &organization,
                    &account,
                )
                .await?;
                print_value(&outcome, cli.json, || {
                    format!(
                        "Synced {} account {}: {} usage buckets, {} cost snapshots.",
                        outcome.provider,
                        outcome.provider_account_id,
                        outcome.usage_buckets,
                        outcome.cost_snapshots
                    )
                })?;
            }
        },
        Command::Integrations { command } => match command {
            IntegrationCommand::List { organization } => {
                let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
                let organization =
                    resolve_organization_id(&cli.api_base_url, &credentials.token, organization)
                        .await?;
                let integrations =
                    list_integrations(&cli.api_base_url, &credentials.token, &organization).await?;
                print_value(&integrations, cli.json, || {
                    integrations
                        .iter()
                        .map(|integration| {
                            format!(
                                "{}\t{}\t{}\t{}\t{}",
                                integration.id,
                                integration.integration_type,
                                integration.display_name,
                                integration.status,
                                integration
                                    .credential_fingerprint
                                    .as_deref()
                                    .unwrap_or("no_secret")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })?;
            }
            IntegrationCommand::Posthog {
                command:
                    PosthogCommand::Connect {
                        organization,
                        name,
                        public_api_url,
                    },
            } => {
                let credentials = require_credentials(&credentials_path, &cli.api_base_url)?;
                let organization =
                    resolve_organization_id(&cli.api_base_url, &credentials.token, organization)
                        .await?;
                let connection =
                    connect_posthog(&cli.api_base_url, &credentials.token, &organization, &name)
                        .await?;
                let webhook_url = format_posthog_webhook_url(
                    public_api_url.as_deref().unwrap_or(&cli.api_base_url),
                    &organization,
                );
                let output = PosthogConnectionOutput {
                    auth_header: format!("Authorization: Bearer {}", connection.webhook_secret),
                    integration: connection.integration,
                    webhook_secret: connection.webhook_secret,
                    webhook_url,
                };
                print_value(&output, cli.json, || {
                    format!(
                        "Connected PostHog integration {}.\nWebhook URL: {}\nAuth header: {}\nSecret: {}\nStore the secret now; Bella only shows it once.",
                        output.integration.id,
                        output.webhook_url,
                        output.auth_header,
                        output.webhook_secret
                    )
                })?;
            }
        },
    }

    Ok(())
}

async fn print_provider_catalog(api_base_url: &str, json: bool) -> anyhow::Result<()> {
    let providers = list_provider_catalog(api_base_url).await?;
    print_value(&providers, json, || {
        providers
            .iter()
            .map(|provider| {
                format!(
                    "{}\t{}\t{}\t{}",
                    provider.id, provider.name, provider.category, provider.ingestion
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

async fn resolve_organization_id(
    api_base_url: &str,
    token: &str,
    organization: Option<String>,
) -> anyhow::Result<String> {
    if let Some(organization) = organization {
        return Ok(organization);
    }

    let organizations = list_organizations(api_base_url, token).await?;
    match organizations.as_slice() {
        [organization] => Ok(organization.id.clone()),
        [] => bail!("no organizations found; create one before connecting providers"),
        _ => {
            let choices = organizations
                .iter()
                .map(|organization| format!("{} ({})", organization.id, organization.name))
                .collect::<Vec<_>>()
                .join(", ");
            bail!("multiple organizations found; pass --organization <id>. Available: {choices}")
        }
    }
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

async fn list_organizations(api_base_url: &str, token: &str) -> anyhow::Result<Vec<Organization>> {
    Client::new()
        .get(format!(
            "{}/v1/organizations",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the organization list request")?
        .json()
        .await
        .context("Bella API returned an invalid organization list")
}

async fn create_organization(
    api_base_url: &str,
    token: &str,
    name: &str,
    idempotency_key: &str,
) -> anyhow::Result<Organization> {
    Client::new()
        .post(format!(
            "{}/v1/organizations",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .header("Idempotency-Key", idempotency_key)
        .json(&CreateOrganizationRequest { name })
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the organization create request")?
        .json()
        .await
        .context("Bella API returned an invalid organization")
}

async fn list_provider_catalog(api_base_url: &str) -> anyhow::Result<Vec<ProviderDefinition>> {
    Client::new()
        .get(format!(
            "{}/v1/providers",
            api_base_url.trim_end_matches('/')
        ))
        .send()
        .await
        .with_context(|| {
            format!(
                "failed to contact Bella API at {}; start it with `just api`",
                api_base_url.trim_end_matches('/')
            )
        })?
        .error_for_status()
        .context("Bella API rejected the provider catalog request")?
        .json()
        .await
        .context("Bella API returned an invalid provider catalog")
}

async fn list_provider_accounts(
    api_base_url: &str,
    token: &str,
    organization_id: &str,
) -> anyhow::Result<Vec<ProviderAccount>> {
    Client::new()
        .get(format!(
            "{}/v1/organizations/{organization_id}/provider-accounts",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the provider account list request")?
        .json()
        .await
        .context("Bella API returned an invalid provider account list")
}

async fn connect_provider(
    api_base_url: &str,
    token: &str,
    organization_id: &str,
    workspace_id: &str,
    provider: &str,
    display_name: &str,
    secret: &str,
) -> anyhow::Result<ProviderAccount> {
    let mut credentials = BTreeMap::new();
    credentials.insert("secret", secret);
    Client::new()
        .post(format!(
            "{}/v1/organizations/{organization_id}/provider-accounts",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .json(&ConnectProviderRequest {
            workspace_id,
            provider,
            display_name,
            credentials,
        })
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the provider connection request")?
        .json()
        .await
        .context("Bella API returned an invalid provider account")
}

async fn disconnect_provider(
    api_base_url: &str,
    token: &str,
    organization_id: &str,
    account_id: &str,
) -> anyhow::Result<()> {
    Client::new()
        .delete(format!(
            "{}/v1/organizations/{organization_id}/provider-accounts/{account_id}",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the provider disconnect request")?;
    Ok(())
}

async fn sync_provider(
    api_base_url: &str,
    token: &str,
    organization_id: &str,
    account_id: &str,
) -> anyhow::Result<SyncOutcome> {
    Client::new()
        .post(format!(
            "{}/v1/organizations/{organization_id}/provider-accounts/{account_id}/sync",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the provider sync request")?
        .json()
        .await
        .context("Bella API returned an invalid provider sync response")
}

async fn list_integrations(
    api_base_url: &str,
    token: &str,
    organization_id: &str,
) -> anyhow::Result<Vec<Integration>> {
    Client::new()
        .get(format!(
            "{}/v1/organizations/{organization_id}/integrations",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the integration list request")?
        .json()
        .await
        .context("Bella API returned an invalid integration list")
}

async fn connect_posthog(
    api_base_url: &str,
    token: &str,
    organization_id: &str,
    display_name: &str,
) -> anyhow::Result<PosthogConnection> {
    Client::new()
        .post(format!(
            "{}/v1/organizations/{organization_id}/integrations/posthog",
            api_base_url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .json(&ConnectPosthogRequest { display_name })
        .send()
        .await
        .context("failed to contact Bella API")?
        .error_for_status()
        .context("Bella API rejected the PostHog integration request")?
        .json()
        .await
        .context("Bella API returned an invalid PostHog integration response")
}

fn format_posthog_webhook_url(public_api_url: &str, organization_id: &str) -> String {
    format!(
        "{}/v1/organizations/{organization_id}/webhooks/posthog",
        public_api_url.trim_end_matches('/')
    )
}

fn read_provider_secret(secret: Option<String>, secret_stdin: bool) -> anyhow::Result<String> {
    if secret.is_some() && secret_stdin {
        bail!("use either --secret/BELLA_PROVIDER_SECRET or --secret-stdin, not both");
    }
    let secret = if secret_stdin {
        let mut value = String::new();
        io::stdin()
            .read_to_string(&mut value)
            .context("failed to read provider credential from standard input")?;
        value
    } else {
        secret.context(
            "provider credential required; set BELLA_PROVIDER_SECRET or use --secret-stdin",
        )?
    };
    let secret = secret.trim().to_owned();
    if secret.is_empty() {
        bail!("provider credential must not be empty");
    }
    Ok(secret)
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

#[cfg(test)]
mod tests {
    use super::read_provider_secret;

    #[test]
    fn provider_secret_rejects_empty_or_conflicting_input() {
        assert!(read_provider_secret(Some("".to_owned()), false).is_err());
        assert!(read_provider_secret(Some("secret".to_owned()), true).is_err());
        assert_eq!(
            read_provider_secret(Some(" secret \n".to_owned()), false).unwrap(),
            "secret"
        );
    }
}
