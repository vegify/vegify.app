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
  }
}
