# Security Policy

## Supported Versions

RayPlay is currently pre-release (alpha). Security fixes are applied to the
latest commit on `main` only.

| Version | Supported |
| ------- | --------- |
| main    | Yes       |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub Issues.**

To report a vulnerability, open a
[GitHub Security Advisory](https://github.com/hakanserce/rayplay/security/advisories/new)
on this repository. This keeps the report private until a fix is available.

Include as much of the following as possible:

- Type of vulnerability (e.g., buffer overflow, credential exposure, MITM)
- Affected component (crate name and file path if known)
- Steps to reproduce or proof-of-concept
- Potential impact

You can expect an acknowledgement within 7 days and a status update within 30 days.

## Security Model

RayPlay is designed for use on a **trusted local network** between machines you
control. The current threat model does **not** cover:

- Untrusted networks or the public internet (no hardened network-level DoS protection)
- Multi-tenant or shared-host deployments
- Adversarial clients connecting to a RayHost without prior pairing

See [ADR-007](docs/adr/ADR-007.md) for the full security design.
