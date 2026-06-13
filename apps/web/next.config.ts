import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  rewrites: async () => [
    {
      source: "/api/:path*",
      destination: "http://127.0.0.1:3000/:path*",
    },
  ],
};

export default nextConfig;
