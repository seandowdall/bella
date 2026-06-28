# Configure the GitHub App Integration

Bella uses GitHub OAuth for user login and a separate GitHub App for repository
access. The GitHub App is what lets Bella agents read repository context and
create pull requests through installation-scoped permissions.

Hosted Bella environments use Bella-owned GitHub Apps. Self-hosted operators
create and own their own app.

## Required URLs

Open **Integrations > GitHub** in the Bella environment you are configuring.
The page shows the exact URLs for that environment.

For local Docker or `just api` defaults:

```text
Setup callback URL: http://127.0.0.1:3000/v1/integrations/github/callback
Webhook URL: http://127.0.0.1:3000/v1/github/webhook
```

For hosted QA and production, use the public API URL for that Railway
environment. Do not reuse local callback or webhook URLs in hosted apps.

## Create the GitHub App

1. Open [GitHub Developer Settings](https://github.com/settings/apps).
2. Select **New GitHub App**.
3. Set **Homepage URL** to the Bella web URL for the environment.
4. Set **Setup URL** to the Bella setup callback URL.
5. Enable **Redirect on update**.
6. Set **Webhook URL** to the Bella webhook URL.
7. Generate a webhook secret and keep it for `BELLA_GITHUB_WEBHOOK_SECRET`.
8. Select **Any account** if this Bella environment should connect customer
   organizations, or **Only on this account** for a single-tenant install.

## Permissions

Start with these repository permissions:

```text
Contents: Read and write
Metadata: Read-only
Pull requests: Read and write
```

Add more permissions only when a product feature needs them. For example,
`Issues: Read and write` is only needed if Bella should create GitHub issues.

## Webhook Events

Subscribe to:

```text
Installation
Installation repositories
```

Bella currently uses these events to keep installation status healthy. Incident
and PR workflows can add more events later.

## Generate a Private Key

In the GitHub App settings, generate a private key and download the `.pem` file.
Store its full contents in your secret manager as `BELLA_GITHUB_PRIVATE_KEY`.
Hosted platforms that do not preserve multiline values can use escaped newline
sequences (`\n`); Bella normalizes both formats at startup.

## Configure Bella

Set these values on the Bella API service for each environment:

```env
BELLA_GITHUB_APP_ID=123456
BELLA_GITHUB_APP_SLUG=bella-your-environment
BELLA_GITHUB_PRIVATE_KEY="-----BEGIN RSA PRIVATE KEY-----\n...\n-----END RSA PRIVATE KEY-----"
BELLA_GITHUB_WEBHOOK_SECRET=...
```

The worker does not need these values until background GitHub jobs are added.
When PR creation moves to the worker, configure the same values there too.

Restart or redeploy the API after changing these values.

## Install the App

1. Open Bella **Integrations > GitHub**.
2. Select **Install GitHub App**.
3. Choose the GitHub organization and repositories Bella may access.
4. Approve the installation.
5. Bella redirects back to the GitHub integration page and refreshes repository
   access.

Bella stores the GitHub installation ID and repository metadata. It does not
store installation access tokens; those are generated on demand and expire at
GitHub.

## Environment Checklist

Use separate GitHub Apps for local development, Bella QA, and Bella production.
Each app should have callback and webhook URLs matching that environment's
`BELLA_PUBLIC_API_URL`.

```text
Local:      http://127.0.0.1:3000
Bella QA:   the QA Railway public API URL
Production: the production Railway public API URL
```

If the install button returns `GitHub App integration is not configured`, the API
is missing one or more `BELLA_GITHUB_*` values.
