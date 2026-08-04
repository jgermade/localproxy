#!/usr/bin/env bash
# Runs the test suite with LLVM source-based coverage and prints a per-file report.
#
# Usage:
#   scripts/coverage.sh              # summary table
#   scripts/coverage.sh --html       # also write target/coverage/html/index.html
#   scripts/coverage.sh --fail-under 70
#
# Requires llvm-profdata/llvm-cov. They ship with rustup's `llvm-tools` component
# and with the Homebrew/apt `llvm` packages. Override the lookup with LLVM_BIN_DIR.

set -euo pipefail

cd "$(dirname "$0")/.."

html=0
fail_under=""
while [ $# -gt 0 ]; do
  case "$1" in
    --html) html=1 ;;
    --fail-under) fail_under="$2"; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

find_llvm_bin_dir() {
  if [ -n "${LLVM_BIN_DIR:-}" ]; then
    echo "$LLVM_BIN_DIR"
    return
  fi

  local rustlib_bin
  rustlib_bin="$(rustc --print target-libdir)/../bin"
  if [ -x "$rustlib_bin/llvm-profdata" ]; then
    echo "$rustlib_bin"
    return
  fi

  if command -v llvm-profdata > /dev/null 2>&1; then
    dirname "$(command -v llvm-profdata)"
    return
  fi

  if command -v brew > /dev/null 2>&1 && [ -x "$(brew --prefix llvm 2>/dev/null)/bin/llvm-profdata" ]; then
    echo "$(brew --prefix llvm)/bin"
    return
  fi

  echo "llvm-profdata not found. Install it with 'rustup component add llvm-tools'," >&2
  echo "'brew install llvm' or 'apt install llvm', or set LLVM_BIN_DIR." >&2
  exit 1
}

llvm_bin="$(find_llvm_bin_dir)"
profdata="$llvm_bin/llvm-profdata"
cov="$llvm_bin/llvm-cov"
out_dir="target/coverage"
ignore='/\.cargo/registry|/rustc/|/rustlib/'

rm -rf "$out_dir"
mkdir -p "$out_dir"

export RUSTFLAGS="${RUSTFLAGS:-} -C instrument-coverage"

# Building for an explicit host target keeps RUSTFLAGS away from build scripts and
# proc-macros, so they do not emit stray .profraw files into the repository root.
host_triple="$(rustc -vV | sed -n 's/^host: //p')"

# Build the instrumented test binaries and collect their paths.
LLVM_PROFILE_FILE="$PWD/$out_dir/build-%p-%m.profraw" \
  cargo test --no-run --target "$host_triple" --message-format=json > "$out_dir/build.json"
rm -f "$out_dir"/build-*.profraw
binaries=()
while IFS= read -r binary; do
  binaries+=("$binary")
done < <(
  python3 - "$out_dir/build.json" <<'PY'
import json
import sys

for line in open(sys.argv[1]):
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if message.get("profile", {}).get("test") and message.get("executable"):
        print(message["executable"])
PY
)

if [ ${#binaries[@]} -eq 0 ]; then
  echo "no test binaries were built" >&2
  exit 1
fi

for binary in "${binaries[@]}"; do
  LLVM_PROFILE_FILE="$PWD/$out_dir/$(basename "$binary")-%p-%m.profraw" "$binary" --quiet
done

"$profdata" merge -sparse "$out_dir"/*.profraw -o "$out_dir/zproxy.profdata"

objects=()
for binary in "${binaries[@]}"; do
  objects+=(--object "$binary")
done

"$cov" report "${objects[@]}" \
  --instr-profile="$out_dir/zproxy.profdata" \
  --ignore-filename-regex="$ignore"

if [ "$html" -eq 1 ]; then
  "$cov" show "${objects[@]}" \
    --instr-profile="$out_dir/zproxy.profdata" \
    --ignore-filename-regex="$ignore" \
    --format=html \
    --output-dir="$out_dir/html"
  echo "html report: $out_dir/html/index.html"
fi

if [ -n "$fail_under" ]; then
  "$cov" export "${objects[@]}" \
    --instr-profile="$out_dir/zproxy.profdata" \
    --ignore-filename-regex="$ignore" \
    --summary-only > "$out_dir/summary.json"

  python3 - "$out_dir/summary.json" "$fail_under" <<'PY'
import json
import sys

summary, minimum = sys.argv[1], float(sys.argv[2])
total = json.load(open(summary))["data"][0]["totals"]["lines"]["percent"]
print(f"line coverage: {total:.2f}% (minimum {minimum:.2f}%)")
if total < minimum:
    print("coverage below the configured minimum", file=sys.stderr)
    sys.exit(1)
PY
fi
