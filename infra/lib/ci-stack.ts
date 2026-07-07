import { CfnOutput, Stack, type StackProps } from "aws-cdk-lib"
import * as iam from "aws-cdk-lib/aws-iam"
import type { Construct } from "constructs"

interface CiStackProps extends StackProps {
  /** "owner/repo" allowed to assume the deploy role via OIDC. */
  githubRepo: string
  /** Secrets Manager id of the shared Apple signing secret the release-signing role may read. */
  appleSecretId: string
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
    super(scope, id, props)

    const provider = iam.OpenIdConnectProvider.fromOpenIdConnectProviderArn(
      this,
      "GithubOidc",
      `arn:aws:iam::${this.account}:oidc-provider/token.actions.githubusercontent.com`
    )

    const role = new iam.Role(this, "DeployRole", {
      roleName: "vegify-github-deploy",
      description:
        "Assumed by GitHub Actions (OIDC) to run cdk deploy via the CDK bootstrap roles.",
      assumedBy: new iam.WebIdentityPrincipal(
        provider.openIdConnectProviderArn,
        {
          StringEquals: {
            "token.actions.githubusercontent.com:aud": "sts.amazonaws.com",
            // Only this repo's workflows on the main branch may assume the role.
            "token.actions.githubusercontent.com:sub": `repo:${props.githubRepo}:ref:refs/heads/main`
          }
        }
      )
    })

    role.addToPolicy(
      new iam.PolicyStatement({
        sid: "AssumeCdkBootstrapRoles",
        actions: ["sts:AssumeRole"],
        resources: [`arn:aws:iam::${this.account}:role/cdk-*`]
      })
    )
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: "ReadCdkBootstrapVersion",
        actions: ["ssm:GetParameter"],
        resources: [`arn:aws:ssm:*:${this.account}:parameter/cdk-bootstrap/*`]
      })
    )
    // deployConfig() reads the /vegify/deploy/* decisions AT SYNTH under this role. Without this
    // grant the read fails and (in CI) the synth aborts — v0.18.0 proved the silent alternative:
    // an AccessDenied that degraded to placeholders deployed example.com email config to prod.
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: "ReadDeployDecisions",
        actions: ["ssm:GetParameter", "ssm:GetParametersByPath"],
        resources: [
          `arn:aws:ssm:${this.region}:${this.account}:parameter/vegify/deploy/*`
        ]
      })
    )

    new CfnOutput(this, "DeployRoleArn", { value: role.roleArn })

    // Release-signing role: assumed by release.yml's publish-desktop job (OIDC) to read the Apple
    // signing secret (Developer ID cert + App Store Connect API key). Least-privilege — it can ONLY
    // GetSecretValue on that one secret, in us-west-1. The secret's name comes in as a prop
    // (props.appleSecretId — kept out of the tree). (One-time bootstrap: `cdk deploy VegifyCi`.)
    const releaseRole = new iam.Role(this, "ReleaseSigningRole", {
      roleName: "vegify-release-signing",
      description:
        "Assumed by GitHub Actions (OIDC) to read the shared Apple signing secret for notarized desktop releases.",
      assumedBy: new iam.WebIdentityPrincipal(
        provider.openIdConnectProviderArn,
        {
          StringEquals: {
            "token.actions.githubusercontent.com:aud": "sts.amazonaws.com",
            // Only this repo's workflows on the main branch may assume the role.
            "token.actions.githubusercontent.com:sub": `repo:${props.githubRepo}:ref:refs/heads/main`
          }
        }
      )
    })
    releaseRole.addToPolicy(
      new iam.PolicyStatement({
        sid: "ReadSharedAppleSigningSecret",
        actions: ["secretsmanager:GetSecretValue"],
        // Secret ARNs carry a random 6-char suffix, hence the trailing wildcard.
        resources: [
          `arn:aws:secretsmanager:us-west-1:${this.account}:secret:${props.appleSecretId}-*`
        ]
      })
    )
    // publish-desktop resolves its two non-secret inputs from Parameter Store instead of repository
    // secrets: the backend origin the binary bakes in (written by VegifyServer) and the Apple signing
    // secret's id. Both live in the deploy region (us-east-1 for vegify).
    releaseRole.addToPolicy(
      new iam.PolicyStatement({
        sid: "ReadDeployParameters",
        actions: ["ssm:GetParameter"],
        resources: [
          `arn:aws:ssm:${this.region}:${this.account}:parameter/vegify/deploy/api-url`,
          `arn:aws:ssm:${this.region}:${this.account}:parameter/vegify/deploy/apple-secret-id`
        ]
      })
    )
    new CfnOutput(this, "ReleaseSigningRoleArn", {
      value: releaseRole.roleArn
    })
  }
}
