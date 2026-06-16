import { Stack, type StackProps } from "aws-cdk-lib";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import type { Construct } from "constructs";

/**
 * One VPC, NO NAT gateway (a NAT is ~$32/mo — avoided). Fargate runs in public subnets
 * with a public IP for egress (image pull); the app Lambdas run in private-isolated
 * subnets and reach AWS services through VPC endpoints instead of the internet.
 */
export class VpcStack extends Stack {
  readonly vpc: ec2.Vpc;

  constructor(scope: Construct, id: string, props: StackProps) {
    super(scope, id, props);

    this.vpc = new ec2.Vpc(this, "Vpc", {
      maxAzs: 2,
      natGateways: 0,
      subnetConfiguration: [
        { name: "public", subnetType: ec2.SubnetType.PUBLIC, cidrMask: 24 },
        { name: "private", subnetType: ec2.SubnetType.PRIVATE_ISOLATED, cidrMask: 24 },
      ],
    });

    // S3 is a free gateway endpoint — OpenNext server/image Lambdas read assets/cache from S3.
    this.vpc.addGatewayEndpoint("S3", { service: ec2.GatewayVpcEndpointAwsService.S3 });

    // Interface endpoints so in-VPC Lambdas can reach these without a NAT (each ~$7/mo;
    // trim any the apps don't actually use at runtime).
    for (const [id, service] of [
      ["SecretsManager", ec2.InterfaceVpcEndpointAwsService.SECRETS_MANAGER],
      ["CloudWatchLogs", ec2.InterfaceVpcEndpointAwsService.CLOUDWATCH_LOGS],
    ] as const) {
      this.vpc.addInterfaceEndpoint(id, { service });
    }
  }
}
