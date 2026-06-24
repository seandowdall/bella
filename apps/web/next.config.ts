import type { NextConfig } from "next";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const appRoot = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = dirname(dirname(appRoot));
const apiOrigin = process.env.BELLA_API_ORIGIN ?? "http://127.0.0.1:3000";

const nextConfig: NextConfig = {
  turbopack: {
    root: workspaceRoot,
  },
  rewrites: async () => [
    {
      source: "/ingest/static/:path*",
      destination: "https://eu-assets.i.posthog.com/static/:path*",
    },
    {
      source: "/ingest/array/:path*",
      destination: "https://eu-assets.i.posthog.com/array/:path*",
    },
    {
      source: "/ingest/:path*",
      destination: "https://eu.i.posthog.com/:path*",
    },
    {
      source: "/api/:path*",
      destination: `${apiOrigin.replace(/\/$/, "")}/:path*`,
    },
  ],
  skipTrailingSlashRedirect: true,
};

export default nextConfig;
