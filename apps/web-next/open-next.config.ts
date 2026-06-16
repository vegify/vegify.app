// OpenNext config — adapts the Next.js build to AWS (Lambda + CloudFront + S3).
// Empty config = defaults: a single server Lambda + image-optimization Lambda + S3 assets.
// Deployed via the repo's `infra/` AWS CDK app. Build with `pnpm --filter web-next build:aws`.
import type { OpenNextConfig } from "@opennextjs/aws/types/open-next.js";

const config: OpenNextConfig = {
  default: {},
};

export default config;
