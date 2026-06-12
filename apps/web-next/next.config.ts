import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  transpilePackages: ["@vegify/ui", "@vegify/db"],
};

export default nextConfig;
