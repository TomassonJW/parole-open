# Security

[English](SECURITY.md) | [Français](SECURITY.fr.md)

Parole is experimental. There is no response-time or supported-version SLA.
Keep model inference local and keep personal data out of issues and patches.

## Report privately

Use this repository's **Security → Report a vulnerability** action when it is
available. Describe the affected version, expected/observed behaviour and a
minimal fictional reproduction. Do not attach real media or transcripts, and
never send a credential, private key or password even in a private report.

Do not open a public issue with a working exploit or sensitive details. If the
private report action is unavailable, ask the maintainer to open a private
channel without disclosing those details. No contact address is invented here.

## Boundaries

Offline processing is not a promise that untrusted media, native parsers or
model outputs are harmless. Keep native tools current, inspect third-party
assets and treat generated text as unverified until reviewed. Explicit model
preparation may require network downloads; it is distinct from inference.

A detected credential must be revoked through its legitimate provider and
removed from future distributions. Deleting a commit or making a repository
private cannot retrieve copies already made. Do not put the value in a report.
