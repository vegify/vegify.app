# Security policy

vegify.app is a continuously deployed application — only the latest release (and the code on `main`) is supported. There are no maintained older versions to backport fixes to: fixes ship forward, usually within the same day they land.

## Reporting a vulnerability

**Email [security@vegify.app](mailto:security@vegify.app).** Any address at the domain reaches the maintainer; this one is reserved for security reports. If GitHub's private vulnerability reporting is enabled on this repository, that channel works too and is equally preferred.

Please include what you'd want in a report you received: affected endpoint/component, reproduction steps or a proof of concept, and your assessment of impact. Encrypting is not required.

What to expect:

- Acknowledgment within **72 hours** (this is a solo-maintained project; usually much faster).
- An honest assessment of severity and a fix timeline. Credential, session, or data-exposure issues in the hosted service get fixed before anything else on the roadmap.
- Credit in the release notes if you want it (or anonymity if you don't).

There is **no bug bounty** — this is a noncommercial project. Good-faith security research against your own self-hosted instance is welcome without reservation. Against the hosted service (vegify.app / api.vegify.app), please keep it non-destructive: no data exfiltration beyond proof of concept, no denial of service, no testing against accounts you don't own — report what you find and it will be taken seriously.

## Scope notes for researchers

- The hosted service is a single small instance behind CloudFront. Volumetric findings ("I can knock over a t4g.nano") are architecture, not vulnerabilities.
- User content is **public by default** by design — recipes and ingredients being readable without an account is intended behavior, not an IDOR. Ownership gates *editing*; anything that lets a non-owner modify or delete content absolutely is in scope.
- Secrets live in AWS (SSM + Secrets Manager), never in this repository or its CI logs. Identifiers you may find in workflow logs (role ARNs, an App Store Connect key id, a Secrets Manager secret *name*) are not credentials.
