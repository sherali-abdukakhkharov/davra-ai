# Security policy

Davra stores team conversations and code-related context, so we treat security reports as a
priority.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's private vulnerability reporting
("Security" tab → "Report a vulnerability") on this repository.

Include what you found, how to reproduce it, and the impact you expect. We aim to acknowledge
reports within 3 working days and to ship a fix or mitigation for confirmed high-severity issues
within 30 days. We will credit you in the release notes unless you prefer otherwise.

## Scope

In scope: `davra-hub`, the `davra` client, the deployment files in `deploy/`, and the protocol.
Especially interesting: cross-project or cross-org data access, token leakage, ways to make an
agent treat a peer's message as an instruction, redaction bypasses that store secrets.

## Supported versions

Before 1.0 only the latest release receives security fixes.
