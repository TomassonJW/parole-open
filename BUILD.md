# Build from source

[English](BUILD.md) | [Français](BUILD.fr.md)

## Prerequisites

- Rust stable supporting edition 2024 and the locked dependencies. The source
  publication and CI use Rust 1.98.1; older toolchains are not certified here.
- Node.js 22 and pnpm **10.34.5**, as pinned in `ui/package.json`.
- Python 3.11 or newer for the publication and XML checks.
- Bash for the verification and native-engine scripts. On Windows use a Bash
  environment suitable for the chosen toolchain, not PowerShell syntax.

Inspect the manifests and scripts before executing them. Dependency installation
can use the network; test inference and media processing do not use cloud APIs.

## Reproduce the source checks

From a complete checkout, run:

```sh
bash scripts/verify.sh
```

The script installs locked interface dependencies with lifecycle scripts disabled,
produces synthetic Rust audio packets, tests the core and optional topic service,
checks Rust format/Clippy and TypeScript, runs frontend and Python tests, builds
the interface and checks publication hygiene. It does **not** download inference
models or build desktop installers. Its temporary fixture directory is reported
at the end; it contains generated test packets, not user media.

With dependencies already cached, refuse network fetching:

```sh
bash scripts/verify.sh --offline
```

A missing cache is a setup error, not a reason to change the lockfiles.
See [TESTING.md](TESTING.md) for test boundaries and ignored model tests.

## Work on the interface

```sh
pnpm --dir ui install --frozen-lockfile --ignore-scripts
pnpm --dir ui dev
```

This starts the interface development server on port 1420. Browser rendering
can help edit the interface, but does not supply Tauri's native commands or
prove a working desktop application. No browser-only production build is offered.

## Prepare a native desktop application

Tauri additionally needs the platform toolchain and libraries described in its
[official prerequisites](https://v2.tauri.app/start/prerequisites/): Windows C++
build tools/WebView2, macOS command-line developer tools, or Linux GTK 3 and
WebKit2GTK 4.1 development packages.

The repository excludes the native resources. Read and inspect:

- `src-tauri/native/README.md` for the required resources;
- `scripts/build-native.sh` for the pinned engine sources/build options;
- `scripts/fetch-diarization-assets.sh` for checked voice resources/notices;
- `MODEL_MANIFEST.json` for pinned application model downloads.

Native preparation is an explicit additional step. It downloads third-party
sources/resources and needs an appropriate toolchain, disk space and a writable
`TMPDIR`. Do not run it as an unexplained part of a documentation contribution.
Verify the licences and runtime libraries of the actual target build.

Once the native resources and prerequisites are present, the checked-in Tauri
layout uses the installed interface CLI:

```sh
cd src-tauri
node ../ui/node_modules/@tauri-apps/cli/tauri.js dev
```

For packaging, replace `dev` with `build --ci --no-sign`. This creates a local
candidate, **not a signed, notarised or validated official installer**. Test its
installation and real operation on the destination OS before distribution.
Respect managed-device protections; never bypass an employer's security policy.
