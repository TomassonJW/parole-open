# Contributing

[English](CONTRIBUTING.md) | [Français](CONTRIBUTING.fr.md)

Thanks for helping make local transcription easier to inspect and improve.
Start with a small, reproducible change rather than an unbounded rewrite.

## A useful contribution

1. Explain the problem and expected result. Open an issue or a focused pull
   request, in English or French, without personal data.
2. Use fictional examples. Never attach a real recording, transcript,
   credential or unknown binary/model asset.
3. Read [BUILD.md](BUILD.md), [TESTING.md](TESTING.md) and the
   [architecture](docs/architecture.md). For a behaviour change, add a failing
   regression before the minimal implementation.
4. Run `bash scripts/verify.sh`, then inspect the staged diff and run
   `python3 scripts/check-staged-secrets.py` before committing. Explain any
   changed test count or ignored test.
5. Update both documentation languages when meaning or commands change. The
   English/French documents are full equivalents, not separate feature promises.

Keep lockfiles and dependencies unchanged unless the change needs them. Inspect
any new component's origin, licence, behaviour and scope before running it.
Preserve transcript/audio originals, explicit cancellation and stale-result
checks. Do not weaken a regression simply to make CI pass.

## Rights and review

Contributions use the component's existing licence: MIT for the main code and
Apache 2.0 in `src-gliclass/`. Submit only work you may license on those terms,
preserve upstream notices and read [LICENSING.md](LICENSING.md). No contributor
agreement, guaranteed merge or response-time commitment is imposed here.

Be respectful, factual and constructive. English and French contributions are
welcome. Maintainers may decline changes outside the local-first scope.
Security issues use [SECURITY.md](SECURITY.md), not a public exploit report.
