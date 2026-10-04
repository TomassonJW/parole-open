# Parole

[English](README.md) · [Français](README.fr.md)

![Parole, a local-first workspace for spoken records](docs/assets/parole-banner.svg)

**From a recording to a document you can inspect.** Parole is a local-first desktop application for transcribing audio and video, separating voices, optionally translating speech, and drafting a report alongside its source passages. Its reader keeps the recording, timed transcript, and presentation controls within the same workflow. Processing runs on local engines, not a remote inference service.

> **Source release, experimental.** This repository provides code for contributors, not an official public installer. The desktop interface and installer text are currently in French. These English and French documents do not imply a translated interface or a supported production release.

## What is here

- Import media, transcribe it in persisted segments, distinguish speakers, and resume interrupted local jobs.
- Read and listen against timed passages; adjust how the transcript is grouped and displayed without rewriting its source segments.
- Request translation and a source-based report with timed quotations. Both are aids for review, not verified records of what was said or decided.
- Explore explicitly prepared **lexical topic leads** and their original passages. This is not automatic, production-calibrated topic classification.
- Export TXT, Markdown, DOCX, JSON, SRT, or VTT, with availability and incomplete-result warnings depending on job state.

Speech, language, and speaker resources are pinned in [`MODEL_MANIFEST.json`](MODEL_MANIFEST.json). Models, real recordings, and native engine binaries are **not** stored in Git. Building native resources and explicitly downloading models can require network access; inference has no cloud fallback.

## Start from source

1. Read [BUILD.md](BUILD.md) for platform prerequisites, native engines, model assets, and the desktop build. A frontend preview alone is not the native processing application.
2. Use [TESTING.md](TESTING.md) for the checks appropriate to your environment. Do not treat a successful source build as installed-app acceptance.
3. Follow the [usage guide](docs/usage.md) only after preparing the required local resources. There is no official public installer to download here.

The equivalent entry points are [BUILD.fr.md](BUILD.fr.md), [TESTING.fr.md](TESTING.fr.md), and the [guide en français](docs/usage.fr.md).

## Documentation

| Read | English | Français |
| --- | --- | --- |
| System boundaries and data flow | [Architecture](docs/architecture.md) | [Architecture](docs/architecture.fr.md) |
| Source setup and application workflow | [Usage](docs/usage.md) | [Utilisation](docs/usage.fr.md) |
| Build and verification | [Build](BUILD.md) · [Testing](TESTING.md) | [Compilation](BUILD.fr.md) · [Tests](TESTING.fr.md) |
| Licensing and included third-party components | [Licensing](LICENSING.md) · [Third-party licenses](THIRD_PARTY_LICENSES.md) | [Licences](LICENSING.fr.md) · [Licences tierces](THIRD_PARTY_LICENSES.fr.md) |
| Contribute and report vulnerabilities | [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) | [Contribuer](CONTRIBUTING.fr.md) · [Sécurité](SECURITY.fr.md) |
| Release policy and changes | [Release](RELEASE.md) · [Changelog](CHANGELOG.md) | [Publication](RELEASE.fr.md) · [Historique](CHANGELOG.fr.md) |

## Boundaries

The repository is not a dataset or a benchmark. Recognition, speaker attribution, translation, and reports need human review before use or sharing. Installed Windows behavior, target Mac acceptance, long meetings, operation on 8 GB of RAM, and real-world translation and report quality are not established as general guarantees. See [RELEASE.md](RELEASE.md) for release criteria rather than assuming a packaged build is validated.

The main code is MIT-licensed; the optional `src-gliclass` service is Apache-2.0. Model and engine licenses are separate: consult [LICENSING.md](LICENSING.md) and [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) before distributing a bundle. Contributions are welcome under [CONTRIBUTING.md](CONTRIBUTING.md); use the private reporting route specified in [SECURITY.md](SECURITY.md) for vulnerabilities, not a public issue containing sensitive details.
