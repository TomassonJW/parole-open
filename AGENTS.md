# Parole contributor rules

- Keep media processing and model inference local. No cloud fallback.
- Never read, display or commit credentials or real user recordings/transcripts.
- Preserve original transcript/audio; view settings and derived results are separate.
- Read README.md, BUILD.md, TESTING.md and LICENSING.md before changes.
- Keep behaviour changes minimal and test-first. Do not weaken tests for green.
- Use the locked dependencies; inspect additions and preserve component notices.
- Run bash scripts/verify.sh and staged-secret checks before committing.
- Do not claim installed/native validation from a browser preview or interface build.
- Documentation is maintained in English and French; desktop UI is currently French.
