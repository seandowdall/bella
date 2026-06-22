import type { NextConfig } from "next";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const appRoot = dirname(fileURLToPath(import.meta.url));
const apiOrigin = process.env.BELLA_API_ORIGIN ?? "http://127.0.0.1:3000";

const nextConfig: NextConfig = {
  turbopack: {
    root: appRoot,
  },
  rewrites: async () => [
    {
      source: "/api/:path*",
      destination: `${apiOrigin.replace(/\/$/, "")}/:path*`,
    },
  ],
};

export default nextConfig;
