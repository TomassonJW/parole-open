# Licensing

[English](LICENSING.md) | [Français](LICENSING.fr.md)

## Original code and documentation

The original Parole code, documentation and artwork are available under the
[MIT licence](LICENSE), **except `src-gliclass/`**, which uses
[Apache 2.0](src-gliclass/LICENSE). Existing copyright notices remain intact.
`src-core` and `src-tauri` declare MIT; the interface package also declares MIT.
Contributions follow the licence of the component they change.

Both licences permit use, modification and redistribution, including commercial
use. MIT does not require a derivative to publish its modifications. The
English licence texts are authoritative; this page explains their scope and
does not replace them. No warranty is provided.

## Dependencies, models and data

External dependencies keep their own licences. The Parole licence does not
relicense FFmpeg, ONNX Runtime, inference engines or model weights. Read
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md),
[MODEL_MANIFEST.json](MODEL_MANIFEST.json) and the upstream notices.

This repository contains approved fictional/synthetic regression fixtures, not
user recordings or real meeting transcripts. Generated audio packets used in
tests stay temporary. Original synthetic fixtures follow the surrounding MIT
licence; third-party material must retain its own notices if added later.

## Source publication is not binary clearance

No model weights, native engine binaries or official desktop installers are
shipped here. A future executable distribution must separately verify the exact
components, notices, source obligations and linking configuration. In
particular, LGPL FFmpeg and MPL-licensed dependencies must not be relabelled
as MIT or Apache. Do not assume that a successful build completes this audit.
