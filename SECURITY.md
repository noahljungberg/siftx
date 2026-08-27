# Security Policy

## Reporting a vulnerability

Please report security vulnerabilities privately rather than through a public
issue, so that a fix can be prepared before details are widely known.

Use GitHub's [private vulnerability
reporting](https://github.com/truespar/siftx/security/advisories/new) for this
repository.

Please include enough detail to reproduce: affected version or commit, a file
that triggers the issue if one exists, and the impact you believe it has.

## What this library's threat model is

SiftX parses untrusted input. Every file it reads - an image from a stranger, a
PDF from an email - is attacker-controlled, and the parsers are the security
boundary. Bugs we consider vulnerabilities:

- A crash, panic, or hang reachable from a malformed file. SiftX is a library:
  a panic takes the host process with it, and an unbounded loop is a denial of
  service in whatever is calling us.
- Any memory-safety failure, including in the C ABI or a language binding.
- Reading beyond the mapped file, or a parser being induced to allocate
  proportional to a length field rather than to data actually present.
- Anything that escapes the parser: a path written outside a caller-supplied
  output directory during image extraction, for instance.

SiftX never executes content it parses, follows no network references, and
resolves no external entities.

## Fuzzing

`fuzz/` holds a libFuzzer target per format family plus a catch-all
auto-detect target. If you are looking for parser bugs, that is the fastest
way in:

```bash
cargo +nightly fuzz run fuzz_pdf
```

Findings from fuzzing are welcome as ordinary issues **unless** they show
memory unsafety, in which case please use private reporting above.

## A note on the unsafe surface

Most of the crate is safe Rust. The exceptions are deliberate and small:
memory-mapped file access, the `extern "C"` layer in `src/ffi/`, and the
lifetime erasure the Python and Node bindings use to hand out a document that
outlives the borrow it was parsed from. Those are the places worth looking
hardest at, and where a report is most valuable.
