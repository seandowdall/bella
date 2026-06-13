# Bella Marketing Site

Single-page Next.js site for `bellalabs.ai`.

The authenticated dashboard remains in `apps/web` and is deployed separately
at `app.bellalabs.ai`.

```sh
just site
```

The local site runs at `http://127.0.0.1:5174`. Override the dashboard link with
`NEXT_PUBLIC_BELLA_APP_URL`.
