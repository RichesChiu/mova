# Security Policy

English · [简体中文](SECURITY.zh-CN.md)

## Report a vulnerability

Do not disclose a suspected vulnerability in a public Issue, Pull Request, discussion, or chat.

Prefer [GitHub private vulnerability reporting](https://github.com/RichesChiu/mova/security/advisories/new). If it is unavailable, email `riches.chiu@gmail.com` with:

- the affected Mova version or image digest;
- the affected endpoint, parser, file type, or deployment path;
- reproducible steps or a minimal proof of concept;
- the expected security impact; and
- any known workaround.

Do not include real credentials, private media, or personal data. The maintainer will acknowledge the report, validate its scope, and coordinate disclosure after a fix or mitigation is available.

## Supported versions

Security fixes target the current stable release. Preview builds are evaluation channels and receive fixes through a newer preview or stable release. Production deployments should use immutable version tags and update to the latest stable patch.

## Container release policy

Official images support Linux `amd64` and `arm64`. Before an immutable release is promoted, the exact candidate manifest is smoke-tested and scanned on both platforms. The release gate:

- refreshes the Debian runtime base without cached package layers;
- reports all critical and high findings;
- blocks fixable critical or high findings;
- blocks findings in the CISA Known Exploited Vulnerabilities catalog; and
- requires explicit review of residual findings without an upstream fix.

A VEX `not_affected` statement requires repository and binary evidence that the vulnerable path is absent or unreachable. Reachable unpatched findings remain visible and must be accepted by exact CVE for that release; broad or silent exceptions are not permitted.

Container scanning reduces risk but cannot make arbitrary media trustworthy. Mount only trusted media, keep Mova and the host runtime updated, and use HTTPS plus an authenticated reverse proxy before exposing the service publicly.
