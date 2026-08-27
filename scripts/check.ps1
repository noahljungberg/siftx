# Run the checks CI would run, locally and for free. Windows equivalent of
# check.sh.
#
#   .\scripts\check.ps1              Rust only
#   .\scripts\check.ps1 -Bindings    also C#, Java, Python and Node.js
#
# Every check is reported pass or fail and the script keeps going, so one
# failure does not hide the rest. Exits non-zero if anything failed.
#
# NOTE: check.sh has been run and is known to work. This PowerShell version is
# a translation of it that has not been executed - there was no Windows machine
# to try it on. Treat the first run as a test of the script as much as of the
# build, and fix it in place if it misbehaves.

param([switch]$Bindings)

Set-Location (Join-Path $PSScriptRoot '..')
$pass = 0; $fail = 0; $skip = 0

function Invoke-Check($Name, $Command) {
    $out = & cmd /c "$Command" 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "  PASS  $Name"; $script:pass++
    } else {
        Write-Host "  FAIL  $Name"; $script:fail++
        $out | Select-Object -Last 15 | ForEach-Object { Write-Host "          $_" }
    }
}
function Skip-Check($Name, $Why) { Write-Host "  SKIP  $Name ($Why not installed)"; $script:skip++ }
function Test-Tool($Exe) { $null -ne (Get-Command $Exe -ErrorAction SilentlyContinue) }

Write-Host "Rust"
Invoke-Check "fmt"                 "cargo fmt --all -- --check"
Invoke-Check "build"               "cargo build --all-features --all-targets"
Invoke-Check "test"                "cargo test --all-features"
Invoke-Check "docs"                "set RUSTDOCFLAGS=-Dwarnings && cargo doc --all-features --no-deps"
Invoke-Check "release build"       "cargo build --release --all-features"
Invoke-Check "benches compile"     "cargo bench --all-features --no-run"
Invoke-Check "no default features" "cargo build --lib --no-default-features"
foreach ($f in @('jpeg','tiff','png','webp','heif','gif','bmp','xmp','iptc','icc','pdf','quicktime','ffi')) {
    Invoke-Check "feature: $f" "cargo build --lib --no-default-features --features $f"
}
Invoke-Check "clippy" "cargo clippy --all-features --all-targets"
if (Test-Tool 'cargo-deny') { Invoke-Check "cargo-deny" "cargo deny check" } else { Skip-Check "cargo-deny" "cargo-deny" }

if ($Bindings) {
    Write-Host ""
    Write-Host "Bindings"
    & cmd /c "cargo build --release --all-features" | Out-Null
    if (Test-Tool 'dotnet') { Invoke-Check "C#" "dotnet test bindings\csharp\tests\SiftX.Tests\SiftX.Tests.csproj" } else { Skip-Check "C#" "dotnet" }
    if (Test-Tool 'mvn')    { Invoke-Check "Java" "cd bindings\java && mvn -B test" }                   else { Skip-Check "Java" "maven" }
    if (Test-Tool 'npm')    { Invoke-Check "Node.js" "cd bindings\nodejs && npm ci && npm run build && npm test" } else { Skip-Check "Node.js" "npm" }
}

Write-Host ""
Write-Host "$pass passed, $fail failed, $skip skipped"
if ($fail -gt 0) { exit 1 }
