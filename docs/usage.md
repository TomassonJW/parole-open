# Using Parole

[English](usage.md) · [Français](usage.fr.md) · [Home](../README.md)

This guide distinguishes **preparing the source** from **using a locally built desktop application**. The code checkout contains neither an official public installer nor the native binaries and models required to process media. The application UI described below is in French.

## Before opening media

For contributors, begin with [BUILD.md](../BUILD.md): it covers platform tooling, the native engines, speaker assets, licensing notices, and building the Tauri application. Then use [TESTING.md](../TESTING.md) for the checks that apply to your platform. Do not substitute a Vite frontend preview for the desktop application: the preview cannot perform native media processing. Build and model preparation may access the network; processing itself uses local inference only.

For application use, ensure the native bundle is complete, the device has enough free storage for the selected models and job data, and you have the right to process the recording. The import screen's model panel checks local readiness. Request model preparation explicitly if needed; the application verifies pinned downloads. Translation and the baseline report require the language model in addition to speech resources. Do not work around a missing-resource warning by substituting unverified files. There is no public installer or universal hardware-performance promise here.

## A recording, step by step

1. **Import.** Select a supported local audio/video file in the application, or drop one onto it. Selection alone does not launch a job. Keep the source available for later listening; the app's retained local-media behavior is not a remote backup.
2. **Choose the work.** Confirm the spoken language or leave automatic detection, optionally choose a translation language, and decide whether to generate a report. If reporting, choose a compatible report language and an installed, available report model. The form explains blocked combinations or missing resources before start.
3. **Run and recover.** Start transcription. The job advances through segmented recognition and any requested translation/report stages; speaker separation may leave some passages unattributed. Review progress in the processing view. An interrupted job can be reopened and resumed; an incomplete result is marked as such, not silently final. Preserve local job data and the source media if you expect to continue or listen later.
4. **Inspect the result.** Read timed original passages and, where requested, the translation and report. Use playback to check disputed words and speaker attribution when the source remains accessible. Assign speaker names only after checking them. A quoted report passage is a way back to evidence, not proof that the surrounding summary is correct.
5. **Adjust the reader.** In **Présentation**, choose fluid or detailed grouping, pause threshold, timestamps, and speaker colors. These affect display and some exports, not the original transcript or recording. Per-job preferences are separate from defaults for future jobs. If a preview or save becomes stale, reload rather than applying it to a different source revision.
6. **Explore only on request.** In **Pistes de sujets**, explicitly prepare lexical candidates for that job, inspect original passages, and return to the transcript or audio. No automatic model-based classification is delivered by this UI; a word match must not be presented as a verified assignment.
7. **Export with the right expectations.** Choose TXT, Markdown, DOCX, JSON, SRT, or VTT. Text, Markdown, and Word can reflect presentation settings; JSON is job data, and subtitles retain original segment timing. Incomplete jobs can export warned text documents, while JSON and subtitles are withheld until recovery. Check the saved file and its contents before distributing it.

## Review and privacy

Local processing does not make transcripts accurate or remove the obligation to handle recordings lawfully. Verify sensitive names, speaker labels, translated meaning, quotations, action owners, and report conclusions against the audio and context. Keep exports and local job data in an appropriately protected location. Model installation or building from pinned sources is an explicit network activity, not network inference on a recording.

For the implementation boundaries, read [architecture](architecture.md). For packaging and redistribution obligations, read [LICENSING.md](../LICENSING.md), [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md), and [RELEASE.md](../RELEASE.md).
