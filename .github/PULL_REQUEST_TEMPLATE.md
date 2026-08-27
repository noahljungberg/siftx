## What this changes

<!-- What the change does, and why. The reasoning matters more than the diff. -->

## How it was verified

<!-- Commands run, or the scenario exercised. For a parser change, say which
     real files you checked the output against. -->

- [ ] `cargo test --all-features`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-features --all-targets` (advisory - no new findings)
- [ ] `cargo deny check`
- [ ] The binding suite, if a binding changed

## Checklist

- [ ] Added or updated tests
- [ ] Updated `README.md` / `docs/` if setup or behaviour changed
- [ ] Regenerated `THIRD-PARTY-NOTICES.md` if dependencies changed
- [ ] If a struct in `include/siftx.h` changed, every binding's layout was
      updated in the same commit
- [ ] No code was copied or transcribed from ExifTool, Poppler, or any other
      GPL-licensed project (see [CONTRIBUTING.md](../CONTRIBUTING.md))
