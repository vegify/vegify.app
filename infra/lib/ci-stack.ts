import { CfnOutput, Stack, type StackProps } from "aws-cdk-lib";
import * as iam from "aws-cdk-lib/aws-iam";
import type { Construct } from "constructs";

interface CiStackProps extends StackProps {
  /** "owner/repo" allowed to assume the deploy role via OIDC. */
  githubRepo: string;
}

/**
 * GitHub Actions deploy role (OIDC — no stored AWS secrets). The `.github/workflows/deploy.yml`
 * workflow assumes this role with a short-lived OIDC token, then runs `cdk deploy`. It is
 * least-privilege: it can only ASSUME the CDK bootstrap roles (which carry the real deploy
 * permissions) and read the bootstrap version — so widening what CI can touch is a bootstrap
 * concern, not a matter of editing this role.
 *
 * The account already has the GitHub OIDC provider, so we reference it rather than create one.
 * One-time: `cdk deploy VegifyCi`; then the workflow uses the DeployRoleArn output.
 */
export class CiStack extends Stack {
  constructor(scope: Construct, id: string, props: CiStackProps) {
    super(scope, id, props);

    const provider = iam.OpenIdConnectProvider.fromOpenIdConnectProviderArn(
      this,
      "GithubOidc",
      `arn:aws:iam::${this.account}:oidc-provider/token.actions.githubusercontent.com`,
    );

    const role = new iam.Role(this, "DeployRole", {
      roleName: "vegify-github-deploy",
      description: "Assumed by GitHub Actions (OIDC) to run cdk deploy via the CDK bootstrap roles.",
      assumedBy: new iam.WebIdentityPrincipal(provider.openIdConnectProviderArn, {
        StringEquals: { "token.actions.githubusercontent.com:aud": "sts.amazonaws.com" },
        StringLike: { "token.actions.githubusercontent.com:sub": `repo:${props.githubRepo}:*` },
      }),
    });

    role.addToPolicy(
      new iam.PolicyStatement({
        sid: "AssumeCdkBootstrapRoles",
        actions: ["sts:AssumeRole"],
        resources: [`arn:aws:iam::${this.account}:role/cdk-*`],
      }),
    );
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: "ReadCdkBootstrapVersion",
        actions: ["ssm:GetParameter"],
        resources: [`arn:aws:ssm:*:${this.account}:parameter/cdk-bootstrap/*`],
      }),
    );

    new CfnOutput(this, "DeployRoleArn", { value: role.roleArn });

    // Release-signing role: assumed by release.yml's publish-desktop job (OIDC) to read the SHARED
    // Apple signing secret (Developer ID cert + App Store Connect API key) reused from
    // johncarmack1984/lux. Least-privilege — it can ONLY GetSecretValue on that one secret, in
    // us-west-1 where lux created it. (One-time bootstrap, like the deploy role: `cdk deploy VegifyCi`.)
    const releaseRole = new iam.Role(this, "ReleaseSigningRole", {
      roleName: "vegify-release-signing",
      description:
        "Assumed by GitHub Actions (OIDC) to read the shared Apple signing secret for notarized desktop releases.",
      assumedBy: new iam.WebIdentityPrincipal(provider.openIdConnectProviderArn, {
        StringEquals: { "token.actions.githubusercontent.com:aud": "sts.amazonaws.com" },
        StringLike: { "token.actions.githubusercontent.com:sub": `repo:${props.githubRepo}:*` },
      }),
    });
    releaseRole.addToPolicy(
      new iam.PolicyStatement({
        sid: "ReadSharedAppleSigningSecret",
        actions: ["secretsmanager:GetSecretValue"],
        // Secret ARNs carry a random 6-char suffix, hence the trailing wildcard.
        resources: [`arn:aws:secretsmanager:us-west-1:${this.account}:secret:your-org/apple-signing-*`],
      }),
    );
    new CfnOutput(this, "ReleaseSigningRoleArn", { value: releaseRole.roleArn });
  }
}
