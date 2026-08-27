//! Regression: R6 (AES-256) authentication against real Acrobat-written
//! corpus files. Before the Algorithm 2.B fix, sift's "R6" was the deprecated
//! R5 single-SHA-256 scheme, so every modern AES-256 file failed
//! authentication - and then FlateDecode ate undecrypted bytes, surfacing as
//! "0 pages" (found via corpus testing).
//!
//! Corpus ground truth (arbitrated against pdfium FPDF_LoadMemDocument and an
//! independent Python 2.B implementation):
//! - empty_protected.pdf: EMPTY user password - must authenticate + decrypt.
//!   This is the known-answer test for the 2.B hardening loop and the 48-byte
//!   /O //U clamps (the file stores 127-byte strings).
//! - print_protection.pdf: REAL password (pdfium error 4) - auth must fail
//!   gracefully, never mis-report success or panic.
//! - bug1815476.pdf: R4/AES-128 empty password - the pre-fix working class
//!   must stay working.

use std::path::Path;

fn parse(path: &str) -> Option<(Vec<u8>, bool)> {
    if !Path::new(path).exists() {
        eprintln!("skipping: {path} not available");
        return None;
    }
    let data = std::fs::read(path).expect("read");
    let doc = siftx::pdf::document::Document::parse(&data).expect("parse");
    let authed = doc.is_authenticated();
    Some((data, authed))
}

#[test]
fn r6_empty_password_authenticates_and_decrypts() {
    let file = "testdata/pdfjs-pdfs/empty_protected.pdf";
    let Some((data, authed)) = parse(file) else {
        return;
    };
    assert!(authed, "{file}: empty-password R6 auth failed");
    let doc = siftx::pdf::document::Document::parse(&data).expect("parse");
    let pages = doc.page_count().expect("page_count decrypts");
    assert!(pages > 0, "{file}: no pages after auth");
    // text extraction must not error either (the page may be blank - that's
    // fine; undecrypted-garbage FlateDecode failures are not)
    doc.pages().expect("pages parse post-decrypt");
}

#[test]
fn r6_real_password_fails_gracefully() {
    let file = "testdata/pdfjs-pdfs/print_protection.pdf";
    let Some((_, authed)) = parse(file) else {
        return;
    };
    assert!(
        !authed,
        "{file}: needs a real password, empty must NOT authenticate"
    );
}

#[test]
fn r4_empty_password_still_authenticates() {
    let Some((_, authed)) = parse("testdata/pdfjs-pdfs/bug1815476.pdf") else {
        return;
    };
    assert!(authed, "R4 empty-password auth regressed");
}
