# Architecture

[English](architecture.md) · [Français](architecture.fr.md) · [Home](../README.md)

Parole is a Tauri desktop application with a React interface and a Rust processing core. Its trust boundary is the local device: the application prepares or verifies pinned resources, runs native engines locally, and persists jobs under the application's local data directory. No remote inference or cloud fallback is part of the processing path. Initial builds and explicitly requested model downloads may use the network.

## Source map

| Location | Responsibility |
| --- | --- |
| `ui/` | React/TypeScript workflow for import, job status, transcript reader, topic leads, presentation settings, and exports. `ui/src/lib/backend.ts` is the frontend/backend boundary. |
| `src-tauri/src/main.rs` | Tauri initialization and the registered commands for models, jobs, playback, lexical topic leads, presentation, and exports. |
| `src-tauri/src/jobs.rs`, `audio.rs`, `models.rs`, `exports.rs` | Job orchestration, retained-audio access, model preparation and checks, and file export. |
| `src-tauri/src/presentation.rs`, `presentation_store.rs` | Preview and persistence of per-job and default view settings. |
| `src-tauri/src/topics.rs` | Explicit read or preparation of lexical candidates for a job. |
| `src-core/src/` | Job and segment types, resumable chunk processing, diarization, translation/report logic, verified-report rendering, transcript projection, and topic candidate/cache logic. |
| `src-gliclass/` | Separate optional local GLiClass classification service (Apache-2.0); not a production UI classification path. |
| `MODEL_MANIFEST.json`, `scripts/`, `src-tauri/native/` | Pinned resource inventory, build/fetch helpers, and expected native bundle layout. Binaries and models are not Git contents. |

## Processing and persistence

A user selects a local file and confirms the spoken language, optional translation, and optional report before starting. Native decoders, speech recognition, and speaker separation operate on local material; speech is processed in chunks. The core `Job` records segments, speaker IDs, requested follow-up work, progress, and state. A chunk is persisted before being counted as completed. On restart, an unfinished running job is marked interrupted; resumption targets unfinished work. A failed translation or missing report is not silently treated as a complete result. The application stores job state locally and exposes a distinct incomplete-result path for reading and export.

Models are not implicit dependencies obtained by uploading media. `src-tauri/src/models.rs` checks local files and their pinned size/digest, and explicit preparation installs the required speech and language models. Speaker resources are part of the native bundle assembled for a build. `MODEL_MANIFEST.json` documents resource identities and download modes. See [BUILD.md](../BUILD.md) for preparing native engines and their licenses, not just the JavaScript frontend.

## Source is not presentation

The timed `Job.segments` and retained media are the source. `src-core/src/transcript_presentation.rs` derives fluid or detailed reading blocks from segments; display and export options do not rewrite the transcription or audio. `presentation_store.rs` keeps preferences outside the job record, with per-job settings and defaults for future jobs. Saves require an expected preference revision; a conflicting or invalid save is rejected. A preview carries a job ID and a revision computed from segment content. `ui/src/hooks/usePresentation.ts` checks this identity and discards late responses after a job or option change. An export must not be mistaken for an edit to source content: TXT, Markdown, and DOCX can use the chosen presentation, while JSON preserves the job data and SRT/VTT use original segment timing.

The reader's playback is anchored to retained local media, not inferred from display blocks. If source media is unavailable, listening may be unavailable even when a stored transcript remains readable. Speaker labels must be checked by a person; absent speaker attribution is left unassigned rather than invented.

## Evidence and limits

The report pipeline (`src-core/src/language.rs`, `report_lifecycle.rs`, `verified_report.rs`) derives text from the job's transcript or translation and renders timed quotations drawn from source segments. It persists intermediate work and ties retained output to source/options identity so stale cached results are not simply reused for changed input. This design does **not** establish that every model-generated interpretation, action owner, translation, or quotation context is factually correct. Review the underlying words and audio before sharing.

Topic preparation (`src-core/src/topic_access.rs`, `src-tauri/src/topics.rs`) is an explicit local lexical operation; the UI can inspect candidates and navigate back to original passages. The UI gates snapshots by job/source revision so candidates from another transcript are not shown as current. The distinct optional `src-gliclass` code is not connected as an automatic production-classification feature of that reader. Lexical overlap is a lead, not a topic assignment or a decision.

## Distribution boundary

A source checkout is not an installed application. Native engines, models, notices, and target-system acceptance must be handled before distributing a package. The desktop and installer strings are French; this bilingual documentation does not localize them. See [usage](usage.md), [licensing](../LICENSING.md), and [release criteria](../RELEASE.md).
