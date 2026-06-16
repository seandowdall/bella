import type { NextConfig } from "next";
import path from "node:path";

const internalApiUrl = process.env.BELLA_INTERNAL_API_URL ?? "http://127.0.0.1:3000";

const nextConfig: NextConfig = {
  output: "standalone",
  outputFileTracingRoot: path.join(process.cwd(), "../.."),
  rewrites: async () => [
    {
      source: "/api/:path*",
      destination: `${internalApiUrl}/:path*`,
    },
  ],
};

export default nextConfig;
