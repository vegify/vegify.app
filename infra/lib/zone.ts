import { PLACEHOLDER_ZONE_ID } from "@vegify/config/deploy";
import * as route53 from "aws-cdk-lib/aws-route53";
import type { Construct } from "constructs";

export interface ZoneSpec {
  /** The zone's domain name (the primary/email domain). */
  zoneName: string;
  /** Whether the domain came from real configuration (vs the fresh-clone placeholder). */
  configured: boolean;
  /** Explicit zone id — skips the lookup entirely (e.g. multiple zones for one name). */
  overrideZoneId?: string;
}

/**
 * Resolve the hosted zone for a domain. An explicit id override wins; a configured real domain is
 * LOOKED UP by name (one Route53 read at synth, cached in cdk.context.json — needs AWS credentials,
 * which every real deploy has); the unconfigured placeholder path never touches AWS, preserving the
 * fresh-clone zero-env `cdk synth` guarantee.
 */
export function resolveZone(
  scope: Construct,
  id: string,
  spec: ZoneSpec,
): route53.IHostedZone {
  if (spec.overrideZoneId) {
    return route53.HostedZone.fromHostedZoneAttributes(scope, id, {
      hostedZoneId: spec.overrideZoneId,
      zoneName: spec.zoneName,
    });
  }
  if (!spec.configured) {
    return route53.HostedZone.fromHostedZoneAttributes(scope, id, {
      hostedZoneId: PLACEHOLDER_ZONE_ID,
      zoneName: spec.zoneName,
    });
  }
  return route53.HostedZone.fromLookup(scope, id, {
    domainName: spec.zoneName,
  });
}
