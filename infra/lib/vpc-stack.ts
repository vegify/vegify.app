import { Stack, type StackProps } from "aws-cdk-lib";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import type { Construct } from "constructs";

/**
 * One VPC, NO NAT gateway (a NAT is ~$32/mo — avoided) and NO paid interface endpoints.
 *
 * The free-tier web stack (WebStartStack) only needs the VPC so its Lambda can mount EFS, which is
 * reached via in-subnet mount targets — no endpoints required (a VPC Lambda's logs are shipped by
 * the Lambda service, not from inside the VPC). Public subnets are kept for the Fargate path
 * (web-start-fargate-stack.ts) to pull images without a NAT when/if revenue justifies switching.
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
  }
}
