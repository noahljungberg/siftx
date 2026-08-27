# Continuous integration (not active)

These two workflow files are **inert**. GitHub only reads workflows from
`.github/workflows/`, so nothing here runs, and nothing here consumes Actions
minutes. They are kept as a record of the checks the project holds itself to,
ready to activate once the build has been walked through by hand on each
platform.

## Running the same checks locally, for free

`scripts/check.sh` (or `scripts/check.ps1` on Windows) runs everything the
`rust` job would, and prints a pass/fail line per check:

```bash
./scripts/check.sh              # Rust only
./scripts/check.sh --bindings   # also C#, Java, Python, Node.js
```

It needs no network and no Actions minutes. The binding checks are skipped
where the toolchain is absent, so it is useful on a machine that only has Rust.

## Activating them later

Move the files and push:

```bash
git mv docs/ci/ci.yml docs/ci/bindings.yml .github/workflows/
```

Read this first, because the cost is not obvious:

- **Private repositories bill Actions minutes, and not at par.** Linux is 1x,
  Windows 2x, macOS **10x**. A single push that fans out across all three
  platforms costs far more than the job count suggests. Public repositories are
  free for standard runners.
- The triggers here are already narrowed to `main`, pull requests, and manual
  dispatch. They were `branches: ["**"]`, which means every push of every
  branch - the reason this directory exists.
- The macOS and Windows jobs are `workflow_dispatch` only. Run them from the
  Actions tab when you want them, rather than on every commit.
- `Swatinem/rust-cache` writes a multi-gigabyte cache per job. On a private
  repository that fills the cache allowance quickly and then thrashes.

## What CI can and cannot check here

The integration suites read corpora from `testdata/`, which is about 2 GB and
not committed. They skip rather than fail when it is absent, so CI proves the
tree builds and the unit tests pass - it does not reproduce the accuracy
figures in the README. Those need the corpora and the reference tools, and are
run locally. See [../testing.md](../testing.md).
