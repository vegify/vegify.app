// Custom::OriginVerifySecret — get-or-create an SSM SecureString holding the origin-verify secret,
// and return its value to CloudFormation so the stack can wire it into the CloudFront custom header
// and the Lambda envs AT DEPLOY TIME. No human ever generates, sees, or transports the secret: it is
// born random in this account on first deploy and lives only in Parameter Store + the deployed
// resources. (CloudFront custom headers can't use {{resolve:secretsmanager}} dynamic references —
// this deploy-time GetAtt is the mechanism that dodges that limitation.)
//
// Lifecycle: Create/Update = get-or-create (idempotent — an existing parameter is returned as-is, so
// the value is stable across deploys); Delete = retain the parameter (a stack teardown shouldn't
// destroy the secret history). Rotate by overwriting the parameter and bumping the RotationNonce
// property (see client-logs-stack.ts) so CloudFormation re-invokes this handler and re-syncs the
// header + envs in one deploy. The response sets NoEcho so the value is masked in CFN event APIs.

import { randomBytes } from "node:crypto";
import { request } from "node:https";
import {
  GetParameterCommand,
  PutParameterCommand,
  SSMClient,
} from "@aws-sdk/client-ssm";

const ssm = new SSMClient({});

function respond(event, context, status, data, reason) {
  const body = JSON.stringify({
    Status: status,
    Reason: reason ?? `See CloudWatch log stream ${context.logStreamName}`,
    PhysicalResourceId:
      event.ResourceProperties?.ParameterName ?? event.LogicalResourceId,
    StackId: event.StackId,
    RequestId: event.RequestId,
    LogicalResourceId: event.LogicalResourceId,
    NoEcho: true,
    Data: data,
  });
  const url = new URL(event.ResponseURL);
  return new Promise((resolve, reject) => {
    const req = request(
      url,
      {
        method: "PUT",
        headers: {
          "content-type": "",
          "content-length": Buffer.byteLength(body),
        },
      },
      (res) => {
        res.resume();
        res.on("end", resolve);
      },
    );
    req.on("error", reject);
    req.write(body);
    req.end();
  });
}

export const handler = async (event, context) => {
  try {
    const name = event.ResourceProperties.ParameterName;
    if (event.RequestType === "Delete") {
      // Retain the parameter: consumers are gone with the stack; the secret costs nothing to keep.
      await respond(event, context, "SUCCESS", {});
      return;
    }
    let value;
    try {
      const got = await ssm.send(
        new GetParameterCommand({ Name: name, WithDecryption: true }),
      );
      value = got.Parameter.Value;
    } catch (e) {
      if (e?.name !== "ParameterNotFound") throw e;
      value = randomBytes(32).toString("hex");
      await ssm.send(
        new PutParameterCommand({
          Name: name,
          Value: value,
          Type: "SecureString",
          Description:
            "vegify origin-verify secret (generated in-account by Custom::OriginVerifySecret)",
        }),
      );
      console.log(`created ${name}`); // the value itself is never logged
    }
    await respond(event, context, "SUCCESS", { Value: value });
  } catch (e) {
    console.error("origin-secret custom resource failed", e?.name, e?.message);
    await respond(event, context, "FAILED", {}, `${e?.name}: ${e?.message}`);
  }
};
