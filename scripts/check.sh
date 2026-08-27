#!/usr/bin/env bash
# Run the checks CI would run, locally and for free.
#
#   ./scripts/check.sh              Rust only
#   ./scripts/check.sh --bindings   also C#, Java, Python and Node.js
#
# Every check is reported pass or fail and the script keeps going, so one
# failure does not hide the rest. Exits non-zero if anything failed.

set -uo pipefail
cd "$(dirname "$0")/.."

pass=0; fail=0; skip=0
run() {                                    # run <name> <command...>
    local name=$1; shift
    if "$@" >/tmp/siftx-check.$$ 2>&1; then
        printf '  PASS  %s\n' "$name"; pass=$((pass+1))
    else
        printf '  FAIL  %s\n' "$name"; fail=$((fail+1))
        sed 's/^/          /' /tmp/siftx-check.$$ | tail -15
    fi
    rm -f /tmp/siftx-check.$$
}
missing() { printf '  SKIP  %s (%s not installed)\n' "$1" "$2"; skip=$((skip+1)); }

echo "Rust"
run "fmt"                cargo fmt --all -- --check
run "build"              cargo build --all-features --all-targets
run "test"               cargo test --all-features
run "docs"               env RUSTDOCFLAGS=-Dwarnings cargo doc --all-features --no-deps
run "release build"      cargo build --release --all-features
run "benches compile"    cargo bench --all-features --no-run
run "no default features" cargo build --lib --no-default-features
for f in jpeg tiff png webp heif gif bmp xmp iptc icc pdf quicktime ffi; do
    run "feature: $f"    cargo build --lib --no-default-features --features "$f"
done
if command -v cargo-clippy >/dev/null 2>&1 || cargo clippy --version >/dev/null 2>&1; then
    run "clippy"         cargo clippy --all-features --all-targets
else missing "clippy" "clippy"; fi
if cargo deny --version >/dev/null 2>&1; then
    run "cargo-deny"     cargo deny check
else missing "cargo-deny" "cargo-deny"; fi
if cargo +nightly --version >/dev/null 2>&1; then
    run "fuzz targets"   env -C fuzz cargo +nightly check --all-targets
else missing "fuzz targets" "nightly toolchain"; fi

if [ "${1:-}" = "--bindings" ]; then
    echo
    echo "Bindings"
    cargo build --release --all-features >/dev/null 2>&1   # bindings link against this

    if dotnet --version >/dev/null 2>&1; then
        run "C#"   dotnet test bindings/csharp/tests/SiftX.Tests/SiftX.Tests.csproj
    else missing "C#" "dotnet"; fi

    if mvn -version >/dev/null 2>&1; then
        run "Java" env -C bindings/java mvn -B test
    else missing "Java" "maven"; fi

    # maturin is commonly only inside the binding's own virtualenv rather than
    # on PATH, so look there before giving up.
    if [ -x bindings/python/.venv/bin/maturin ]; then
        run "Python" bash -c "cd bindings/python && .venv/bin/maturin develop --release -q && .venv/bin/python -m pytest -q"
    elif command -v maturin >/dev/null 2>&1; then
        run "Python" bash -c "cd bindings/python && maturin develop --release && python -m pytest -q"
    else
        printf '  SKIP  Python (no maturin; create the venv with:\n'
        printf '          python3 -m venv bindings/python/.venv \\\n'
        printf '            && bindings/python/.venv/bin/pip install "maturin>=1.14,<2" pytest)\n'
        skip=$((skip+1))
    fi

    if command -v npm >/dev/null 2>&1; then
        run "Node.js" bash -c "cd bindings/nodejs && npm ci && npm run build && npm test"
    else missing "Node.js" "npm"; fi
fi

echo
printf '%d passed, %d failed, %d skipped\n' "$pass" "$fail" "$skip"
[ "$fail" -eq 0 ]
