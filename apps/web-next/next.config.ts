import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  transpilePackages: ["@vegify/ui", "@vegify/db"],
  // React Compiler (stable in Next 16, opt-in). Auto-memoizes components; Turbopack runs an
  // SWC pre-check so only files that need it take the Babel pass. Needs babel-plugin-react-compiler.
  reactCompiler: true,
};

export default nextConfig;
