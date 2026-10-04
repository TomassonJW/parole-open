# Tests and evidence

[English](TESTING.md) | [Français](TESTING.fr.md)

Run `bash scripts/verify.sh` from the repository root, or add `--offline` when
all dependency caches are ready. See [BUILD.md](BUILD.md) for prerequisites.
The script is the reproducible entry point; individual commands remain visible
in `scripts/verify.sh`.

## What the source checks cover

- Core state, cancellation/recovery, retained audio, cited-report validation,
  transcript presentation and exports.
- Local topic worker protocol, identity checks, limits and stale-result handling.
- React behaviours, reader controls and Rust-produced audio transport envelopes.
- Publication checks: forbidden paths without opening their contents, common
  credential patterns without displaying values, private references, file size,
  symbolic links and local documentation links.
- Python checks for synthetic fixtures, signing evidence formats and Word XML.

The script creates `PAROLE_AUDIO_FIXTURE_OUTPUT` in a fresh temporary directory
**before** running Rust, then keeps the same variable for frontend tests. This
exercises transport tests that would otherwise skip. The audio is synthetic.
Do not replace these envelopes with invented JSON or real meeting recordings.

## What a green run does not prove

Model-backed tests marked ignored need explicit local resources and are not run
by this source-only suite. A passing unit suite does not prove real model
quality, long-meeting performance, an 8 GB memory budget or installed desktop
operation on Windows/macOS. The interface build is not native acceptance.
Translation, speaker assignment and source-grounded reports still require human
review. A citation does not make an inferred conclusion true.

The publication checker is a defence-in-depth heuristic, not a mathematical
proof that every possible secret is absent. Review provenance, new binary assets
and licence obligations as well. Never submit a credential to test a detector.

## Change discipline

For a behaviour change: write the failing regression, observe the intended
failure, fix minimally, rerun the focused and full relevant suites, then commit
a small reviewed change. Preserve the original transcript/audio and don't
weaken a test to obtain green. Explain changes in collected/skipped test counts.
Only fictional data with redistribution rights may be committed.

Fixtures referenced by Rust via `include_str!` must remain at their paths,
including `ui/tests/fixtures/` and the four approved states in
`docs/evaluations/orion-2026-09-28/`. That directory is a narrow regression input,
not an archive of real meetings or private delivery evidence.
