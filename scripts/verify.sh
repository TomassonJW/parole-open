#!/usr/bin/env bash
# Source-only checks. Native engines and real inference models are deliberately excluded.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
cargo_flags=(--locked)
pnpm_flags=(--frozen-lockfile --ignore-scripts)
if [[ "${1:-}" == "--offline" && $# -eq 1 ]]; then
  cargo_flags+=(--offline)
  pnpm_flags+=(--offline)
elif [[ $# -ne 0 ]]; then
  printf 'Usage: bash scripts/verify.sh [--offline]\n' >&2
  exit 2
fi
work=$(mktemp -d "${TMPDIR:-/tmp}/parole-verify.XXXXXX")
# Rust emits synthetic binary envelopes; frontend tests must consume the same packets.
export PAROLE_AUDIO_FIXTURE_OUTPUT="$work/audio-packet.bin"
for component in src-core src-gliclass; do
  cargo test --manifest-path "$component/Cargo.toml" "${cargo_flags[@]}"
  cargo fmt --manifest-path "$component/Cargo.toml" --check
  cargo clippy --manifest-path "$component/Cargo.toml" "${cargo_flags[@]}" --all-targets -- -D warnings
done
pnpm --dir ui install "${pnpm_flags[@]}"
pnpm --dir ui typecheck
pnpm --dir ui test
pnpm --dir ui build
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
python3 scripts/check-publication.py
python3 - <<'PY'
import hashlib, json, pathlib, tomllib
root = pathlib.Path.cwd()
assert (root/'LICENSE').read_text().startswith('MIT License\n')
assert hashlib.sha256((root/'src-gliclass/LICENSE').read_bytes()).hexdigest() == 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30'
for component, expected in [('src-core', 'MIT'), ('src-tauri', 'MIT'), ('src-gliclass', 'Apache-2.0')]:
    assert tomllib.loads((root/component/'Cargo.toml').read_text())['package']['license'] == expected
package = json.loads((root/'ui/package.json').read_text())
assert package['license'] == 'MIT' and package['private'] is True
print('PASS: licence texts and component metadata')
PY
printf 'PASS: source checks. Generated test packets retained at %s\n' "$work"
