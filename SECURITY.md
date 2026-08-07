# Security Policy

## Reporting a vulnerability

Do not disclose a suspected vulnerability in a public Issue, Pull Request, discussion, or chat.
Use GitHub private vulnerability reporting when it is available. Otherwise, email
`riches.chiu@gmail.com` with:

- the affected Mova version or image digest;
- the affected endpoint, parser, file type, or deployment path;
- reproducible steps or a minimal proof of concept;
- the expected security impact; and
- any known workaround.

Avoid including real credentials, private media, or personal data. The maintainer will acknowledge
the report, validate its scope, and coordinate disclosure after a fix or mitigation is available.

## Supported versions

Security fixes target the current stable release. Preview builds are evaluation channels and may
receive fixes only through a newer preview or stable release. Users should run immutable version
tags in production and update to the newest stable patch release.

## Container release policy

Official images are built for Linux `amd64` and `arm64`. Before an immutable release tag is
promoted, the exact candidate manifest is smoke-tested and scanned on both platforms.

The release gate:

- refreshes the Debian runtime base without cached package layers;
- reports all critical and high findings;
- blocks every fixable critical or high finding;
- blocks every finding in the CISA Known Exploited Vulnerabilities catalog; and
- requires explicit review of every residual finding without an upstream fix.

A VEX `not_affected` statement is allowed only when repository and binary evidence shows that the
vulnerable code path is absent or unreachable. An unpatched finding that may be reachable must stay
visible and be accepted by its exact CVE identifier for that release. Broad or silent vulnerability
exceptions are not permitted.

Container scanning reduces risk but does not make arbitrary media trustworthy. Administrators
should mount only media they trust, keep Mova and the host runtime updated, and avoid exposing the
service directly to the public internet without an authenticated reverse proxy and transport
security.
