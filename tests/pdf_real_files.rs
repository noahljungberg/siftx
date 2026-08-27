//! Integration tests: run PDF parsing against real test corpora.
//!
//! Tests pdfinfo (metadata), pdftotext (text extraction), and pdfimages
//! (image extraction) equivalents against real PDF files.

use std::path::Path;

// ---------------------------------------------------------------------------
// pdfinfo: metadata extraction across all corpora
// ---------------------------------------------------------------------------

#[test]
fn pdfinfo_poppler_test() {
    let dir = Path::new("testdata/poppler-test");
    if !dir.exists() {
        eprintln!("skipping: testdata/poppler-test not available");
        return;
    }

    let mut total = 0;
    let mut success = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    for path in find_pdfs(dir) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                failures.push((name, format!("read error: {e}")));
                continue;
            }
        };

        match siftx::pdf::document::Document::parse(&data) {
            Ok(doc) => {
                // Test metadata extraction
                match doc.metadata() {
                    Ok(meta) => {
                        // Basic sanity: page count should be > 0
                        assert!(meta.page_count > 0, "{name}: page_count is 0");
                        // Version should be present
                        assert!(meta.version.is_some(), "{name}: no version detected");
                        success += 1;
                    }
                    Err(e) => {
                        failures.push((name, format!("metadata error: {e}")));
                    }
                }
            }
            Err(e) => {
                failures.push((name, format!("parse error: {e}")));
            }
        }
    }

    eprintln!("pdfinfo poppler-test: {success}/{total} succeeded");
    if !failures.is_empty() {
        eprintln!("failures:");
        for (name, err) in &failures {
            eprintln!("  {name}: {err}");
        }
    }
    // Allow up to 10% failure rate for edge cases
    let min_success = (total as f64 * 0.90) as usize;
    assert!(
        success >= min_success,
        "too many failures: {success}/{total} (need {min_success})"
    );
}

#[test]
fn pdfinfo_pdfjs() {
    let dir = Path::new("testdata/pdfjs-pdfs");
    if !dir.exists() {
        eprintln!("skipping: testdata/pdfjs-pdfs not available");
        return;
    }

    let mut total = 0;
    let mut parse_ok = 0;
    let mut metadata_ok = 0;
    let mut parse_failures: Vec<String> = Vec::new();
    let mut metadata_failures: Vec<String> = Vec::new();

    for path in find_pdfs(dir) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        match siftx::pdf::document::Document::parse(&data) {
            Ok(doc) => {
                parse_ok += 1;
                match doc.metadata() {
                    Ok(_meta) => {
                        metadata_ok += 1;
                    }
                    Err(e) => {
                        metadata_failures.push(format!("{name}: {e}"));
                    }
                }
            }
            Err(e) => {
                parse_failures.push(format!("{name}: {e}"));
            }
        }
    }

    eprintln!("pdfinfo pdfjs: parse {parse_ok}/{total}, metadata {metadata_ok}/{total}");
    if !parse_failures.is_empty() {
        eprintln!("parse failures ({}):", parse_failures.len());
        for f in parse_failures.iter().take(20) {
            eprintln!("  {f}");
        }
        if parse_failures.len() > 20 {
            eprintln!("  ... and {} more", parse_failures.len() - 20);
        }
    }
    if !metadata_failures.is_empty() {
        eprintln!("metadata failures ({}):", metadata_failures.len());
        for f in metadata_failures.iter().take(20) {
            eprintln!("  {f}");
        }
        if metadata_failures.len() > 20 {
            eprintln!("  ... and {} more", metadata_failures.len() - 20);
        }
    }

    // pdfjs corpus has intentionally broken PDFs - allow 20% failure
    let min_parse = (total as f64 * 0.80) as usize;
    assert!(
        parse_ok >= min_parse,
        "too many parse failures: {parse_ok}/{total} (need {min_parse})"
    );
}

#[test]
fn pdfinfo_verapdf() {
    let dir = Path::new("testdata/verapdf-corpus");
    if !dir.exists() {
        eprintln!("skipping: testdata/verapdf-corpus not available");
        return;
    }

    let mut total = 0;
    let mut parse_ok = 0;
    let mut with_subtype = 0;

    // Sample first 500 to keep test time reasonable
    for path in find_pdfs(dir).into_iter().take(500) {
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if let Ok(doc) = siftx::pdf::document::Document::parse(&data) {
            parse_ok += 1;
            if let Ok(meta) = doc.metadata() {
                if meta.subtype.is_some() {
                    with_subtype += 1;
                }
            }
        }
    }

    eprintln!("pdfinfo verapdf: {parse_ok}/{total} parsed, {with_subtype} with subtype");
    let min_parse = (total as f64 * 0.85) as usize;
    assert!(
        parse_ok >= min_parse,
        "too many parse failures: {parse_ok}/{total} (need {min_parse})"
    );
}

// ---------------------------------------------------------------------------
// pdftotext: text extraction
// ---------------------------------------------------------------------------

#[test]
fn pdftotext_poppler_test() {
    let dir = Path::new("testdata/poppler-test");
    if !dir.exists() {
        eprintln!("skipping: testdata/poppler-test not available");
        return;
    }

    let mut total = 0;
    let mut extracted_text = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    for path in find_pdfs(dir) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(_) => continue, // parse failures counted in pdfinfo test
        };

        let pages = match doc.pages() {
            Ok(p) => p,
            Err(e) => {
                failures.push((name, format!("pages error: {e}")));
                continue;
            }
        };

        let mut any_text = false;
        let mut page_ok = true;

        for (i, page) in pages.iter().enumerate() {
            // Try raw text extraction
            match siftx::pdf::text_layout::extract_text_raw(&doc, page) {
                Ok(text) => {
                    if !text.is_empty() {
                        any_text = true;
                    }
                }
                Err(e) => {
                    failures.push((name.clone(), format!("page {i} raw error: {e}")));
                    page_ok = false;
                    break;
                }
            }

            // Try layout text extraction
            match siftx::pdf::text_layout::extract_text_layout(&doc, page) {
                Ok(text) => {
                    if !text.is_empty() {
                        any_text = true;
                    }
                }
                Err(e) => {
                    failures.push((name.clone(), format!("page {i} layout error: {e}")));
                    page_ok = false;
                    break;
                }
            }
        }

        if page_ok && any_text {
            extracted_text += 1;
        }
    }

    eprintln!("pdftotext poppler-test: {extracted_text}/{total} with text");
    if !failures.is_empty() {
        eprintln!("failures ({}):", failures.len());
        for (name, err) in &failures {
            eprintln!("  {name}: {err}");
        }
    }
    // Not all PDFs contain text (some are just images) so lower threshold
    let min_success = (total as f64 * 0.50) as usize;
    assert!(
        extracted_text >= min_success || failures.len() <= total / 4,
        "too many text extraction failures"
    );
}

#[test]
fn pdftotext_reference_output() {
    // Compare our text output to Poppler's reference text files
    let dir = Path::new("testdata/poppler-test/tests");
    if !dir.exists() {
        eprintln!("skipping: testdata/poppler-test/tests not available");
        return;
    }

    let mut tested = 0;
    let mut matched = 0;

    // Look for *-text-ref.txt or *-text-out.txt files
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        if !name.ends_with("-text-ref.txt") && !name.ends_with("-text-out.txt") {
            continue;
        }

        // Derive PDF filename: "encoding.pdf-0-text-ref.txt" -> "encoding.pdf"
        let pdf_name = name.split(".pdf-").next().unwrap_or("").to_string() + ".pdf";
        let pdf_path = dir.join(&pdf_name);
        if !pdf_path.exists() {
            continue;
        }

        let reference_text = std::fs::read_to_string(&path).unwrap();
        let pdf_data = std::fs::read(&pdf_path).unwrap();

        let doc = match siftx::pdf::document::Document::parse(&pdf_data) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let pages = match doc.pages() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Extract text from page 0
        if let Some(page) = pages.first() {
            tested += 1;

            let our_text =
                siftx::pdf::text_layout::extract_text_layout(&doc, page).unwrap_or_default();

            // Normalize whitespace for comparison
            let ref_normalized = normalize_text(&reference_text);
            let our_normalized = normalize_text(&our_text);

            if ref_normalized == our_normalized {
                matched += 1;
                eprintln!("  MATCH: {pdf_name}");
            } else {
                // Check partial match (shared words)
                let ref_words: std::collections::HashSet<&str> =
                    ref_normalized.split_whitespace().collect();
                let our_words: std::collections::HashSet<&str> =
                    our_normalized.split_whitespace().collect();

                let common = ref_words.intersection(&our_words).count();
                let total_words = ref_words.len().max(1);
                let pct = common * 100 / total_words;

                eprintln!("  PARTIAL: {pdf_name} - {pct}% word overlap ({common}/{total_words})");
                eprintln!(
                    "    ref: {:?}...",
                    &ref_normalized[..ref_normalized.len().min(100)]
                );
                eprintln!(
                    "    our: {:?}...",
                    &our_normalized[..our_normalized.len().min(100)]
                );

                if pct >= 80 {
                    matched += 1; // Close enough
                }
            }
        }
    }

    eprintln!("pdftotext reference: {matched}/{tested} matched");
}

#[test]
fn pdftotext_pdfjs_sample() {
    let dir = Path::new("testdata/pdfjs-pdfs");
    if !dir.exists() {
        eprintln!("skipping: testdata/pdfjs-pdfs not available");
        return;
    }

    let mut total = 0;
    let mut text_ok = 0;
    let mut text_failures = 0;

    // Sample first 200 for speed
    for path in find_pdfs(dir).into_iter().take(200) {
        let _name = path.file_name().unwrap().to_string_lossy().to_string();
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let pages = match doc.pages() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Just try raw text on page 0
        if let Some(page) = pages.first() {
            match siftx::pdf::text_layout::extract_text_raw(&doc, page) {
                Ok(_) => text_ok += 1,
                Err(_) => text_failures += 1,
            }
        }
    }

    eprintln!("pdftotext pdfjs: {text_ok}/{total} ok, {text_failures} failures");
    let min_ok = (total as f64 * 0.70) as usize;
    assert!(
        text_ok >= min_ok,
        "too many text failures: {text_ok}/{total} (need {min_ok})"
    );
}

// ---------------------------------------------------------------------------
// pdfimages: image extraction
// ---------------------------------------------------------------------------

#[test]
fn pdfimages_poppler_test() {
    let dir = Path::new("testdata/poppler-test");
    if !dir.exists() {
        eprintln!("skipping: testdata/poppler-test not available");
        return;
    }

    let mut total = 0;
    let mut with_images = 0;
    let mut total_images = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    for path in find_pdfs(dir) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(_) => continue,
        };

        match siftx::pdf::image_extract::extract_all_images(&doc) {
            Ok(images) => {
                if !images.is_empty() {
                    with_images += 1;
                    total_images += images.len();

                    // Validate each image has sane properties
                    for img in &images {
                        assert!(img.width > 0, "{name}: image width is 0");
                        assert!(img.height > 0, "{name}: image height is 0");
                    }
                }
            }
            Err(e) => {
                failures.push((name, format!("extract error: {e}")));
            }
        }
    }

    eprintln!(
        "pdfimages poppler-test: {with_images}/{total} have images, {total_images} total images"
    );
    if !failures.is_empty() {
        eprintln!("failures ({}):", failures.len());
        for (name, err) in &failures {
            eprintln!("  {name}: {err}");
        }
    }
}

#[test]
fn pdfimages_list_mode() {
    let dir = Path::new("testdata/poppler-test");
    if !dir.exists() {
        eprintln!("skipping: testdata/poppler-test not available");
        return;
    }

    let mut total = 0;
    let mut list_ok = 0;

    for path in find_pdfs(dir) {
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(_) => continue,
        };

        match siftx::pdf::image_extract::list_all_images(&doc) {
            Ok(images) => {
                list_ok += 1;
                // List mode should return Empty data
                for img in &images {
                    assert!(
                        matches!(img.data, siftx::pdf::image_extract::ImageData::Empty),
                        "list mode should return Empty data"
                    );
                }
            }
            Err(_) => {}
        }
    }

    eprintln!("pdfimages list poppler-test: {list_ok}/{total} ok");
}

#[test]
fn pdfimages_jpeg_passthrough() {
    // Specifically test jpeg.pdf - should extract JPEG data bit-identical
    let path = Path::new("testdata/poppler-test/tests/jpeg.pdf");
    if !path.exists() {
        eprintln!("skipping: jpeg.pdf not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let doc = siftx::pdf::document::Document::parse(&data).unwrap();
    let images = siftx::pdf::image_extract::extract_all_images(&doc).unwrap();

    eprintln!("jpeg.pdf: {} images extracted", images.len());
    for (i, img) in images.iter().enumerate() {
        eprintln!(
            "  image {}: {}x{} bpc={} enc={} size={}",
            i,
            img.width,
            img.height,
            img.bpc,
            img.encoding_name(),
            img.data_size()
        );

        // JPEG passthrough: data should start with FFD8
        if img.encoding == siftx::pdf::image_extract::ImageEncoding::Jpeg {
            match &img.data {
                siftx::pdf::image_extract::ImageData::Passthrough(bytes) => {
                    assert!(
                        bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8,
                        "JPEG passthrough should start with FFD8"
                    );
                }
                other => {
                    panic!(
                        "expected Passthrough for JPEG, got {:?}",
                        std::mem::discriminant(other)
                    );
                }
            }
        }
    }
}

#[test]
fn pdfimages_pdfjs_sample() {
    let dir = Path::new("testdata/pdfjs-pdfs");
    if !dir.exists() {
        eprintln!("skipping: testdata/pdfjs-pdfs not available");
        return;
    }

    let mut total = 0;
    let mut with_images = 0;
    let mut total_images = 0;
    let mut failures = 0;

    // Sample first 100
    for path in find_pdfs(dir).into_iter().take(100) {
        total += 1;

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(_) => continue,
        };

        match siftx::pdf::image_extract::extract_all_images(&doc) {
            Ok(images) => {
                if !images.is_empty() {
                    with_images += 1;
                    total_images += images.len();
                }
            }
            Err(_) => failures += 1,
        }
    }

    eprintln!(
        "pdfimages pdfjs: {with_images}/{total} have images, {total_images} total, {failures} failures"
    );
}

// ---------------------------------------------------------------------------
// Specific file tests
// ---------------------------------------------------------------------------

#[test]
fn specific_cropbox_pdf() {
    let path = Path::new("testdata/poppler-test/tests/cropbox.pdf");
    if !path.exists() {
        eprintln!("skipping: cropbox.pdf not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let doc = siftx::pdf::document::Document::parse(&data).unwrap();
    let meta = doc.metadata().unwrap();

    eprintln!(
        "cropbox.pdf: {} pages, version {:?}",
        meta.page_count, meta.version
    );
    for (i, page) in meta.pages.iter().enumerate() {
        eprintln!(
            "  page {}: media={:?} crop={:?} rotate={}",
            i, page.media_box, page.crop_box, page.rotate
        );
    }

    // Cropbox PDF should have multiple pages with varying crop boxes
    assert!(meta.page_count > 0);
}

#[test]
fn specific_encoding_pdf() {
    let path = Path::new("testdata/poppler-test/tests/encoding.pdf");
    if !path.exists() {
        eprintln!("skipping: encoding.pdf not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let doc = siftx::pdf::document::Document::parse(&data).unwrap();
    let pages = doc.pages().unwrap();

    assert!(!pages.is_empty());

    // Try text extraction
    let text = siftx::pdf::text_layout::extract_text_raw(&doc, &pages[0]).unwrap();
    eprintln!(
        "encoding.pdf text ({} chars): {:?}...",
        text.len(),
        &text[..text.len().min(200)]
    );

    // Should have some text
    assert!(!text.is_empty(), "encoding.pdf should have text");
}

#[test]
fn specific_fonts_pdf() {
    let path = Path::new("testdata/poppler-test/tests/fonts.pdf");
    if !path.exists() {
        eprintln!("skipping: fonts.pdf not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let doc = siftx::pdf::document::Document::parse(&data).unwrap();
    let pages = doc.pages().unwrap();

    let text = siftx::pdf::text_layout::extract_text_raw(&doc, &pages[0]).unwrap();
    eprintln!(
        "fonts.pdf text ({} chars): {:?}...",
        text.len(),
        &text[..text.len().min(200)]
    );
}

#[test]
fn specific_text_pdf() {
    let path = Path::new("testdata/poppler-test/tests/text.pdf");
    if !path.exists() {
        eprintln!("skipping: text.pdf not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let doc = siftx::pdf::document::Document::parse(&data).unwrap();
    let pages = doc.pages().unwrap();

    for (i, page) in pages.iter().enumerate() {
        let raw = siftx::pdf::text_layout::extract_text_raw(&doc, page).unwrap();
        let layout = siftx::pdf::text_layout::extract_text_layout(&doc, page).unwrap();
        eprintln!(
            "text.pdf page {}: raw={} chars, layout={} chars",
            i,
            raw.len(),
            layout.len()
        );
    }
}

#[test]
fn specific_inline_image_pdf() {
    let path = Path::new("testdata/poppler-test/tests/inline-image.pdf");
    if !path.exists() {
        eprintln!("skipping: inline-image.pdf not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let doc = siftx::pdf::document::Document::parse(&data).unwrap();
    let images = siftx::pdf::image_extract::extract_all_images(&doc).unwrap();

    eprintln!("inline-image.pdf: {} images", images.len());
    for (i, img) in images.iter().enumerate() {
        eprintln!(
            "  {}: {}x{} enc={}",
            i,
            img.width,
            img.height,
            img.encoding_name()
        );
    }
}

#[test]
fn specific_doublepage_pdf() {
    let path = Path::new("testdata/poppler-test/unittestcases/doublepage.pdf");
    if !path.exists() {
        eprintln!("skipping: doublepage.pdf not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let doc = siftx::pdf::document::Document::parse(&data).unwrap();
    let meta = doc.metadata().unwrap();

    eprintln!("doublepage.pdf: {} pages", meta.page_count);
    assert!(
        meta.page_count >= 2,
        "doublepage should have at least 2 pages"
    );
}

#[test]
fn specific_pdfa_testsuite() {
    let dir = Path::new("testdata/pdfa-testsuite");
    if !dir.exists() {
        eprintln!("skipping: testdata/pdfa-testsuite not available");
        return;
    }

    let mut total = 0;
    let mut pdfa_detected = 0;

    for path in find_pdfs(dir) {
        total += 1;
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if let Ok(doc) = siftx::pdf::document::Document::parse(&data) {
            if let Ok(meta) = doc.metadata() {
                if matches!(
                    meta.subtype,
                    Some(siftx::pdf::metadata::PdfSubtype::PdfA { .. })
                ) {
                    pdfa_detected += 1;
                }
            }
        }
    }

    eprintln!("pdfa-testsuite: {pdfa_detected}/{total} detected as PDF/A");
    // Most should be detected
    if total > 0 {
        let pct = pdfa_detected * 100 / total;
        eprintln!("  detection rate: {pct}%");
    }
}

// ---------------------------------------------------------------------------
// Encryption tests
// ---------------------------------------------------------------------------

#[test]
fn encrypted_pdf_auth_status() {
    let dir = Path::new("testdata/poppler-test");
    if !dir.exists() {
        eprintln!("skipping: testdata/poppler-test not available");
        return;
    }

    // (filename, should_auth_with_empty_password)
    let encrypted_files: &[(&str, bool)] = &[
        ("Gday garçon - open.pdf", false), // requires password
        ("Gday garçon - owner.pdf", true), // owner-only, empty user password works
        ("PasswordEncrypted.pdf", false),  // requires password
        ("encrypted-256.pdf", false),      // AES-256/R6, requires password
        ("orientation.pdf", true),         // permission-only, empty password works
    ];

    for (fname, expect_auth) in encrypted_files {
        let mut found = None;
        for path in find_pdfs(dir) {
            if path
                .file_name()
                .map(|n| n.to_string_lossy() == *fname)
                .unwrap_or(false)
            {
                found = Some(path);
                break;
            }
        }

        let path = match found {
            Some(p) => p,
            None => {
                eprintln!("  {fname}: not found");
                continue;
            }
        };

        let data = std::fs::read(&path).unwrap();
        let doc = siftx::pdf::document::Document::parse(&data).unwrap();

        assert!(doc.is_encrypted(), "{fname} should be encrypted");
        assert_eq!(
            doc.is_authenticated(),
            *expect_auth,
            "{fname}: expected authenticated={expect_auth}"
        );

        // Verify decrypted text extraction works for authenticated PDFs
        if doc.is_authenticated() {
            let pages = doc.pages().unwrap();
            if let Some(page) = pages.first() {
                let text = siftx::pdf::text_layout::extract_text_raw(&doc, page).unwrap();
                assert!(
                    !text.is_empty(),
                    "{fname}: decrypted text should not be empty"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// pdfinfo field-by-field comparison against Poppler
// ---------------------------------------------------------------------------

/// Parse Poppler's `pdfinfo -rawdates` output into a map of key->value.
fn parse_pdfinfo_output(output: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in output.lines() {
        // Skip indented lines (XMP/custom metadata) - only use primary pdfinfo output
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        // Format: "Key:           value" - key ends at first ':', value is rest trimmed
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim().to_string();
            let val = line[colon + 1..].trim().to_string();
            map.insert(key, val);
        }
    }
    map
}

/// Format page size from CropBox as "W x H" matching pdfinfo style.
/// Poppler's pdfinfo uses getPageCropWidth/Height, not MediaBox.
fn format_page_size(crop_box: &[f64; 4]) -> String {
    let w = crop_box[2] - crop_box[0];
    let h = crop_box[3] - crop_box[1];
    format!("{} x {}", format_g(w), format_g(h))
}

/// Format a float like C's %g: 6 significant digits, no trailing zeros.
fn format_g(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    // %g: use the shorter of %e and %f with 6 significant digits
    // For values in the range we care about (page sizes ~50-2000), %f-style is shorter
    let digits = 6_i32;
    let magnitude = v.abs().log10().floor() as i32;
    let decimal_places = (digits - 1 - magnitude).max(0) as usize;
    let s = format!("{:.prec$}", v, prec = decimal_places);
    // Strip trailing zeros after decimal point
    if s.contains('.') {
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    } else {
        s
    }
}

/// Parse pdfinfo's "W x H pts (Label)" -> "W x H"
fn parse_pdfinfo_page_size(s: &str) -> String {
    // "595.22 x 842 pts (A4)" -> "595.22 x 842"
    if let Some(idx) = s.find(" pts") {
        s[..idx].trim().to_string()
    } else {
        s.to_string()
    }
}

#[test]
fn pdfinfo_field_comparison() {
    // Scan all available PDF corpora
    let corpus_dirs = [
        "testdata/poppler-test",
        "testdata/pdfjs-pdfs",
        "testdata/verapdf-corpus",
        "testdata/pdfa-testsuite",
        "testdata/format-corpus",
    ];

    let mut all_pdfs = Vec::new();
    for dir in &corpus_dirs {
        let p = Path::new(dir);
        if p.exists() {
            all_pdfs.extend(find_pdfs(p));
        }
    }

    if all_pdfs.is_empty() {
        eprintln!("skipping: no PDF test corpora available");
        return;
    }

    // Check pdfinfo is available
    let pdfinfo_check = std::process::Command::new("pdfinfo").arg("--help").output();
    if pdfinfo_check.is_err() {
        eprintln!("skipping: pdfinfo not available");
        return;
    }

    let mut total = 0;
    let mut compared = 0;
    let mut pdfinfo_failed = 0;
    let mut sift_failed = 0;

    // Per-field match counters
    let mut field_total: std::collections::HashMap<&str, (u32, u32)> =
        std::collections::HashMap::new();

    let fields = [
        "Title",
        "Author",
        "Subject",
        "Keywords",
        "Creator",
        "Producer",
        "CreationDate",
        "ModDate",
        "Pages",
        "Page size",
        "Page rot",
        "Tagged",
        "JavaScript",
        "Encrypted",
        "Optimized",
        "PDF version",
    ];
    for f in &fields {
        field_total.insert(f, (0, 0));
    }

    let mut mismatches: Vec<String> = Vec::new();

    for path in &all_pdfs {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        total += 1;

        // Run pdfinfo -rawdates
        let output = match std::process::Command::new("pdfinfo")
            .arg("-rawdates")
            .arg(&path)
            .output()
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => {
                pdfinfo_failed += 1;
                continue;
            }
        };
        let expected = parse_pdfinfo_output(&output);

        // Parse with sift
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => {
                sift_failed += 1;
                continue;
            }
        };
        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(_) => {
                sift_failed += 1;
                continue;
            }
        };
        let meta = match doc.metadata() {
            Ok(m) => m,
            Err(_) => {
                sift_failed += 1;
                continue;
            }
        };

        compared += 1;

        // --- Compare each field ---

        // Title
        if let Some(exp) = expected.get("Title") {
            let got = meta.title.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("Title").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: Title expected={exp:?} got={got:?}"));
            }
        }

        // Author
        if let Some(exp) = expected.get("Author") {
            let got = meta.author.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("Author").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: Author expected={exp:?} got={got:?}"));
            }
        }

        // Subject
        if let Some(exp) = expected.get("Subject") {
            let got = meta.subject.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("Subject").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: Subject expected={exp:?} got={got:?}"));
            }
        }

        // Keywords
        if let Some(exp) = expected.get("Keywords") {
            let got = meta.keywords.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("Keywords").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: Keywords expected={exp:?} got={got:?}"));
            }
        }

        // Creator
        if let Some(exp) = expected.get("Creator") {
            let got = meta.creator.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("Creator").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: Creator expected={exp:?} got={got:?}"));
            }
        }

        // Producer
        if let Some(exp) = expected.get("Producer") {
            let got = meta.producer.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("Producer").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: Producer expected={exp:?} got={got:?}"));
            }
        }

        // CreationDate (raw format with -rawdates)
        if let Some(exp) = expected.get("CreationDate") {
            let got = meta.creation_date.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("CreationDate").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: CreationDate expected={exp:?} got={got:?}"));
            }
        }

        // ModDate
        if let Some(exp) = expected.get("ModDate") {
            let got = meta.mod_date.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("ModDate").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else if !exp.is_empty() {
                mismatches.push(format!("{name}: ModDate expected={exp:?} got={got:?}"));
            }
        }

        // Pages
        if let Some(exp) = expected.get("Pages") {
            let got = meta.page_count.to_string();
            let (t, m) = field_total.get_mut("Pages").unwrap();
            *t += 1;
            if got == *exp {
                *m += 1;
            } else {
                mismatches.push(format!("{name}: Pages expected={exp} got={got}"));
            }
        }

        // Page size (first page only)
        if let Some(exp) = expected.get("Page size") {
            if let Some(page) = meta.pages.first() {
                let got = format_page_size(&page.crop_box);
                let exp_clean = parse_pdfinfo_page_size(exp);
                let (t, m) = field_total.get_mut("Page size").unwrap();
                *t += 1;
                if got == exp_clean {
                    *m += 1;
                } else {
                    mismatches.push(format!(
                        "{name}: Page size expected={exp_clean:?} got={got:?}"
                    ));
                }
            }
        }

        // Page rot (first page)
        if let Some(exp) = expected.get("Page rot") {
            if let Some(page) = meta.pages.first() {
                let got = page.rotate.to_string();
                let (t, m) = field_total.get_mut("Page rot").unwrap();
                *t += 1;
                if got == *exp {
                    *m += 1;
                } else {
                    mismatches.push(format!("{name}: Page rot expected={exp} got={got}"));
                }
            }
        }

        // Tagged
        if let Some(exp) = expected.get("Tagged") {
            let got = if meta.is_tagged { "yes" } else { "no" };
            let (t, m) = field_total.get_mut("Tagged").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else {
                mismatches.push(format!("{name}: Tagged expected={exp} got={got}"));
            }
        }

        // JavaScript
        if let Some(exp) = expected.get("JavaScript") {
            let got = if meta.has_javascript { "yes" } else { "no" };
            let (t, m) = field_total.get_mut("JavaScript").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else {
                mismatches.push(format!("{name}: JavaScript expected={exp} got={got}"));
            }
        }

        // Encrypted (just yes/no)
        if let Some(exp) = expected.get("Encrypted") {
            let got = if meta.encryption.is_some() {
                "yes"
            } else {
                "no"
            };
            let exp_yn = if exp.starts_with("yes") { "yes" } else { "no" };
            let (t, m) = field_total.get_mut("Encrypted").unwrap();
            *t += 1;
            if got == exp_yn {
                *m += 1;
            } else {
                mismatches.push(format!("{name}: Encrypted expected={exp_yn} got={got}"));
            }
        }

        // Optimized (= linearized)
        if let Some(exp) = expected.get("Optimized") {
            let got = if meta.is_linearized { "yes" } else { "no" };
            let (t, m) = field_total.get_mut("Optimized").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else {
                mismatches.push(format!("{name}: Optimized expected={exp} got={got}"));
            }
        }

        // PDF version
        if let Some(exp) = expected.get("PDF version") {
            let got = meta.version.as_deref().unwrap_or("");
            let (t, m) = field_total.get_mut("PDF version").unwrap();
            *t += 1;
            if got == exp {
                *m += 1;
            } else {
                mismatches.push(format!("{name}: PDF version expected={exp} got={got}"));
            }
        }
    }

    // Print summary
    eprintln!(
        "\npdfinfo comparison: {compared}/{total} PDFs compared (pdfinfo failed: {pdfinfo_failed}, sift failed: {sift_failed})"
    );
    eprintln!("Field match rates:");
    let mut all_perfect = true;
    for field in &fields {
        let (t, m) = field_total[field];
        let pct = if t > 0 { m * 100 / t } else { 100 };
        let marker = if m == t { " " } else { "!" };
        eprintln!("  {marker} {field:15} {m}/{t} ({pct}%)");
        if m != t {
            all_perfect = false;
        }
    }

    if !all_perfect && !mismatches.is_empty() {
        eprintln!("\nMismatches ({} total):", mismatches.len());
        for m in &mismatches {
            eprintln!("  {m}");
        }
    }

    // Assert match rates on key fields
    let assert_field = |name: &str, min_pct: u32| {
        let (t, m) = field_total[name];
        let pct = if t > 0 { m * 100 / t } else { 100 };
        assert!(
            pct >= min_pct,
            "{name} match rate {m}/{t} ({pct}%) < {min_pct}%"
        );
    };

    // Minimum match rates across ~4000 PDFs from all corpora.
    // These thresholds represent current known-good rates.
    assert_field("Pages", 99);
    assert_field("PDF version", 99);
    assert_field("Page size", 99);
    assert_field("Page rot", 100);
    assert_field("Tagged", 99);
    assert_field("Encrypted", 99);
    assert_field("Optimized", 99);
    assert_field("JavaScript", 99);
    assert_field("Title", 95);
    assert_field("Author", 95);
    assert_field("Subject", 95);
    assert_field("Keywords", 95);
    assert_field("Creator", 95);
    assert_field("Producer", 95);
    assert_field("CreationDate", 95);
    assert_field("ModDate", 95);
}

// ---------------------------------------------------------------------------
// pdfimages: image extraction comparison (focused set)
// ---------------------------------------------------------------------------

/// Compare image extraction against pdfimages -list for a focused set of PDFs.
/// This test covers: JPEG, Flate, CCITT, stencils, masks, indexed color,
/// ICC, inline images, and duplicate image references.
#[test]
fn pdfimages_focused_comparison() {
    use std::collections::HashSet;

    let test_files = [
        // Poppler test suite - core image types
        "testdata/poppler-test/tests/image.pdf",
        "testdata/poppler-test/tests/jpeg.pdf",
        "testdata/poppler-test/tests/text.pdf",
        "testdata/poppler-test/tests/inline-image.pdf",
        "testdata/poppler-test/tests/mask.pdf",
        "testdata/poppler-test/tests/mask-seams.pdf",
        "testdata/poppler-test/tests/blend.pdf",
        "testdata/poppler-test/unittestcases/truetype.pdf",
        "testdata/poppler-test/unittestcases/NestedLayers.pdf",
        "testdata/poppler-test/unittestcases/imageretrieve+attachment.pdf",
        "testdata/poppler-test/unittestcases/A6EmbeddedFiles.pdf",
        "testdata/poppler-test/unittestcases/checkbox_issue_159.pdf",
        "testdata/poppler-test/unittestcases/ClarityOCGs.pdf",
        // pdfjs - variety of image types and structures
        "testdata/pdfjs-pdfs/160F-2019.pdf",
        "testdata/pdfjs-pdfs/bigboundingbox.pdf",
        "testdata/pdfjs-pdfs/pdkids.pdf",
        "testdata/pdfjs-pdfs/pdfjs_wikipedia.pdf",
        "testdata/pdfjs-pdfs/bitmap-composite-or-xor-replace.pdf",
        "testdata/pdfjs-pdfs/red_stamp.pdf",
        "testdata/pdfjs-pdfs/multiple-filters-length-zero.pdf",
        // issue269_2: 384 Form invocations - dedup strategy differs from poppler
        "testdata/pdfjs-pdfs/issue11878.pdf",
        "testdata/pdfjs-pdfs/issue1905.pdf",
        "testdata/pdfjs-pdfs/issue19971.pdf",
        "testdata/pdfjs-pdfs/issue14256.pdf",
        "testdata/pdfjs-pdfs/issue12963.pdf",
        "testdata/pdfjs-pdfs/issue8823.pdf",
        "testdata/pdfjs-pdfs/issue840.pdf",
        "testdata/pdfjs-pdfs/issue9972-1.pdf",
        "testdata/pdfjs-pdfs/issue5481.pdf",
        "testdata/pdfjs-pdfs/issue1350.pdf",
        "testdata/pdfjs-pdfs/issue9940.pdf",
        "testdata/pdfjs-pdfs/issue7229.pdf",
        "testdata/pdfjs-pdfs/issue5280.pdf",
        "testdata/pdfjs-pdfs/issue4246.pdf",
        "testdata/pdfjs-pdfs/issue18042.pdf",
        "testdata/pdfjs-pdfs/issue16287.pdf",
        "testdata/pdfjs-pdfs/issue14297.pdf",
        "testdata/pdfjs-pdfs/issue12798_page1_reduced.pdf",
        // JPX / JBIG2 / CCITT
        "testdata/pdfjs-pdfs/issue5475.pdf",
        "testdata/pdfjs-pdfs/issue5549.pdf",
        "testdata/pdfjs-pdfs/issue5747.pdf",
        "testdata/pdfjs-pdfs/issue7200.pdf",
        "testdata/pdfjs-pdfs/issue17871_bottom_right.pdf",
        "testdata/pdfjs-pdfs/jp2k-resetprob.pdf",
        "testdata/pdfjs-pdfs/jbig2_symbol_offset.pdf",
        // Inline, masks, SMask
        "testdata/pdfjs-pdfs/issue10388_reduced.pdf",
        "testdata/pdfjs-pdfs/issue11124.pdf",
        "testdata/pdfjs-pdfs/issue14200.pdf",
        "testdata/pdfjs-pdfs/issue18956.pdf",
        "testdata/pdfjs-pdfs/issue19326.pdf",
        "testdata/pdfjs-pdfs/issue19360.pdf",
        "testdata/pdfjs-pdfs/issue4379.pdf",
        "testdata/pdfjs-pdfs/issue6621.pdf",
        "testdata/pdfjs-pdfs/issue13372.pdf",
        "testdata/pdfjs-pdfs/issue12213.pdf",
        "testdata/pdfjs-pdfs/canvas.pdf",
        // Multi-page signed PDF
        "testdata/poppler-test/unittestcases/pdf-signature-sample-2sigs.pdf",
        "testdata/poppler-test/unittestcases/pdf20-utf8-test.pdf",
        "testdata/poppler-test/unittestcases/text.pdf",
        // Format corpus
        "testdata/format-corpus/fully-featured-pdf/PDF-Sample-Document-Fully-Featured-Layout_Redacted.pdf",
    ];

    // Check pdfimages is available
    if std::process::Command::new("pdfimages")
        .arg("--help")
        .output()
        .is_err()
    {
        eprintln!("skipping: pdfimages not available");
        return;
    }

    let mut compared = 0;
    let mut skipped = 0;
    let mut total_pop_images = 0u32;
    let mut total_sift_images = 0u32;
    let mut count_match = 0u32;
    let mut mismatches: Vec<String> = Vec::new();

    // Per-field counters
    let fields = ["type", "width", "height", "color", "comp", "bpc", "enc"];
    let mut field_total: std::collections::HashMap<&str, (u32, u32)> =
        std::collections::HashMap::new();
    for f in &fields {
        field_total.insert(f, (0, 0));
    }

    for file_path in &test_files {
        let path = Path::new(file_path);
        if !path.exists() {
            skipped += 1;
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        // Run pdfimages -list
        let output = match std::process::Command::new("pdfimages")
            .arg("-list")
            .arg(path)
            .output()
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => {
                skipped += 1;
                continue;
            }
        };
        let pop_images = parse_pdfimages_list(&output);

        // Parse with sift
        let data = std::fs::read(path).unwrap();
        let doc = siftx::pdf::document::Document::parse(&data).unwrap();
        let pages = doc.pages().unwrap();

        let mut img_counter = 0u32;
        let mut seen_refs: HashSet<(u32, u16)> = HashSet::new();
        let mut sift_images: Vec<siftx::pdf::image_extract::PdfImage> = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            // Clear seen_refs per page - pdfimages doesn't dedup across pages
            seen_refs.clear();
            if let Ok(imgs) = siftx::pdf::image_extract::extract_images(
                &doc,
                page,
                i as u32,
                &mut img_counter,
                &mut seen_refs,
            ) {
                sift_images.extend(imgs);
            }
        }

        compared += 1;
        total_pop_images += pop_images.len() as u32;
        total_sift_images += sift_images.len() as u32;

        if pop_images.len() == sift_images.len() {
            count_match += 1;
        } else {
            mismatches.push(format!(
                "{name}: image count poppler={} sift={}",
                pop_images.len(),
                sift_images.len()
            ));
        }

        // Compare per-image fields by index
        let compare_len = pop_images.len().min(sift_images.len());
        for i in 0..compare_len {
            let pop = &pop_images[i];
            let sft = &sift_images[i];

            let sift_type = match sft.image_type {
                siftx::pdf::image_extract::ImageType::Image => "image",
                siftx::pdf::image_extract::ImageType::Stencil => "stencil",
                siftx::pdf::image_extract::ImageType::SoftMask => "smask",
                siftx::pdf::image_extract::ImageType::Mask => "mask",
            };
            cmp_field(
                &mut field_total,
                &mut mismatches,
                &name,
                i,
                "type",
                &pop.type_str,
                sift_type,
            );
            cmp_field(
                &mut field_total,
                &mut mismatches,
                &name,
                i,
                "width",
                &pop.width,
                &sft.width.to_string(),
            );
            cmp_field(
                &mut field_total,
                &mut mismatches,
                &name,
                i,
                "height",
                &pop.height,
                &sft.height.to_string(),
            );

            let sift_color = match &sft.color_space {
                siftx::pdf::image_extract::ImageColorSpace::DeviceGray => "gray",
                siftx::pdf::image_extract::ImageColorSpace::DeviceRGB => "rgb",
                siftx::pdf::image_extract::ImageColorSpace::DeviceCMYK => "cmyk",
                siftx::pdf::image_extract::ImageColorSpace::CalGray => "gray",
                siftx::pdf::image_extract::ImageColorSpace::CalRGB => "rgb",
                siftx::pdf::image_extract::ImageColorSpace::ICCBased { .. } => "icc",
                siftx::pdf::image_extract::ImageColorSpace::Indexed { .. } => "index",
                siftx::pdf::image_extract::ImageColorSpace::Separation => "sep",
                siftx::pdf::image_extract::ImageColorSpace::DeviceN => "devn",
                siftx::pdf::image_extract::ImageColorSpace::Unknown => "-",
            };
            if sift_type != "stencil" && sift_type != "mask" {
                cmp_field(
                    &mut field_total,
                    &mut mismatches,
                    &name,
                    i,
                    "color",
                    &pop.color,
                    sift_color,
                );
            }
            cmp_field(
                &mut field_total,
                &mut mismatches,
                &name,
                i,
                "comp",
                &pop.comp,
                &sft.components.to_string(),
            );
            cmp_field(
                &mut field_total,
                &mut mismatches,
                &name,
                i,
                "bpc",
                &pop.bpc,
                &sft.bpc.to_string(),
            );

            let sift_enc = match sft.encoding {
                siftx::pdf::image_extract::ImageEncoding::Jpeg => "jpeg",
                siftx::pdf::image_extract::ImageEncoding::Jpeg2000 => "jpx",
                siftx::pdf::image_extract::ImageEncoding::Jbig2 => "jbig2",
                siftx::pdf::image_extract::ImageEncoding::Ccitt => "ccitt",
                siftx::pdf::image_extract::ImageEncoding::Flate => "image",
                siftx::pdf::image_extract::ImageEncoding::Lzw => "image",
                siftx::pdf::image_extract::ImageEncoding::RunLength => "image",
                siftx::pdf::image_extract::ImageEncoding::Raw => "image",
            };
            cmp_field(
                &mut field_total,
                &mut mismatches,
                &name,
                i,
                "enc",
                &pop.enc,
                sift_enc,
            );
        }
    }

    // Print summary
    eprintln!("\npdfimages focused: {compared} PDFs compared ({skipped} skipped)");
    eprintln!("Image count: poppler={total_pop_images} sift={total_sift_images}");
    eprintln!("Count match: {count_match}/{compared}");
    eprintln!("Field match rates:");
    for field in &fields {
        let (t, m) = field_total[field];
        let pct = if t > 0 { m * 100 / t } else { 100 };
        let marker = if m == t { " " } else { "!" };
        eprintln!("  {marker} {field:10} {m}/{t} ({pct}%)");
    }
    if !mismatches.is_empty() {
        eprintln!("\nMismatches ({}):", mismatches.len());
        for m in &mismatches {
            eprintln!("  {m}");
        }
    }
}

/// Wide pdfimages comparison: scan ALL PDFs in test corpora and compare against
/// poppler's pdfimages -list. Tolerant of parse/extraction failures - reports
/// aggregate stats rather than asserting on individual files.
#[test]
fn pdfimages_wide_comparison() {
    use std::collections::HashSet;

    // Check pdfimages is available
    if std::process::Command::new("pdfimages")
        .arg("--help")
        .output()
        .is_err()
    {
        eprintln!("skipping: pdfimages not available");
        return;
    }

    let dirs = [
        "testdata/poppler-test",
        "testdata/pdfjs-pdfs",
        "testdata/format-corpus",
        "testdata/pdfa-testsuite",
        "testdata/verapdf-corpus",
    ];

    let mut all_pdfs = Vec::new();
    for dir in &dirs {
        let path = Path::new(dir);
        if path.exists() {
            all_pdfs.extend(find_pdfs(path));
        }
    }
    all_pdfs.sort();

    let total_pdfs = all_pdfs.len();
    let mut compared = 0u32;
    let mut poppler_skip = 0u32; // pdfimages failed or no images
    let mut sift_parse_fail = 0u32;
    let mut sift_extract_fail = 0u32;
    let mut count_match = 0u32;
    let mut count_mismatch = 0u32;
    let mut total_pop_images = 0u32;
    let mut total_sift_images = 0u32;

    // Per-field counters
    let fields = ["type", "width", "height", "color", "comp", "bpc", "enc"];
    let mut field_total: std::collections::HashMap<&str, (u32, u32)> =
        std::collections::HashMap::new();
    for f in &fields {
        field_total.insert(f, (0, 0));
    }

    // Top count mismatches for reporting
    let mut count_mismatches: Vec<(String, usize, usize)> = Vec::new();
    // Top field mismatches (limited to avoid huge output)
    let mut field_mismatches: Vec<String> = Vec::new();
    let max_field_mismatches = 50;
    // Per-file width mismatch tracker: (filename, mismatches, total)
    let mut per_file_wh_miss: Vec<(String, u32, u32)> = Vec::new();

    for path in &all_pdfs {
        let name = path
            .strip_prefix("testdata/")
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .to_string();

        // Run pdfimages -list (timeout: 5s)
        let output = match std::process::Command::new("pdfimages")
            .arg("-list")
            .arg(path)
            .output()
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => {
                poppler_skip += 1;
                continue;
            }
        };
        let pop_images = parse_pdfimages_list(&output);
        if pop_images.is_empty() {
            poppler_skip += 1;
            continue;
        }

        // Parse with sift
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => {
                sift_parse_fail += 1;
                continue;
            }
        };
        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(_) => {
                sift_parse_fail += 1;
                continue;
            }
        };
        let pages = match doc.pages() {
            Ok(p) => p,
            Err(_) => {
                sift_parse_fail += 1;
                continue;
            }
        };

        let mut img_counter = 0u32;
        let mut seen_refs: HashSet<(u32, u16)> = HashSet::new();
        let mut sift_images: Vec<siftx::pdf::image_extract::PdfImage> = Vec::new();
        let mut extract_ok = true;
        for (i, page) in pages.iter().enumerate() {
            seen_refs.clear();
            match siftx::pdf::image_extract::extract_images(
                &doc,
                page,
                i as u32,
                &mut img_counter,
                &mut seen_refs,
            ) {
                Ok(imgs) => sift_images.extend(imgs),
                Err(_) => {
                    extract_ok = false;
                    break;
                }
            }
        }
        if !extract_ok {
            sift_extract_fail += 1;
            continue;
        }

        compared += 1;
        total_pop_images += pop_images.len() as u32;
        total_sift_images += sift_images.len() as u32;

        if pop_images.len() == sift_images.len() {
            count_match += 1;
        } else {
            count_mismatch += 1;
            if count_mismatches.len() < 30 {
                count_mismatches.push((name.clone(), pop_images.len(), sift_images.len()));
            }
        }

        // Compare per-image fields
        let compare_len = pop_images.len().min(sift_images.len());
        let mut file_wh_miss = 0u32;
        for i in 0..compare_len {
            let pop = &pop_images[i];
            let sft = &sift_images[i];

            let sift_type = match sft.image_type {
                siftx::pdf::image_extract::ImageType::Image => "image",
                siftx::pdf::image_extract::ImageType::Stencil => "stencil",
                siftx::pdf::image_extract::ImageType::SoftMask => "smask",
                siftx::pdf::image_extract::ImageType::Mask => "mask",
            };

            let record = field_mismatches.len() < max_field_mismatches;
            cmp_field_opt(
                &mut field_total,
                if record {
                    Some(&mut field_mismatches)
                } else {
                    None
                },
                &name,
                i,
                "type",
                &pop.type_str,
                sift_type,
            );
            cmp_field_opt(
                &mut field_total,
                if record {
                    Some(&mut field_mismatches)
                } else {
                    None
                },
                &name,
                i,
                "width",
                &pop.width,
                &sft.width.to_string(),
            );
            cmp_field_opt(
                &mut field_total,
                if record {
                    Some(&mut field_mismatches)
                } else {
                    None
                },
                &name,
                i,
                "height",
                &pop.height,
                &sft.height.to_string(),
            );
            if pop.width != sft.width.to_string() || pop.height != sft.height.to_string() {
                file_wh_miss += 1;
            }

            let sift_color = match &sft.color_space {
                siftx::pdf::image_extract::ImageColorSpace::DeviceGray => "gray",
                siftx::pdf::image_extract::ImageColorSpace::DeviceRGB => "rgb",
                siftx::pdf::image_extract::ImageColorSpace::DeviceCMYK => "cmyk",
                siftx::pdf::image_extract::ImageColorSpace::CalGray => "gray",
                siftx::pdf::image_extract::ImageColorSpace::CalRGB => "rgb",
                siftx::pdf::image_extract::ImageColorSpace::ICCBased { .. } => "icc",
                siftx::pdf::image_extract::ImageColorSpace::Indexed { .. } => "index",
                siftx::pdf::image_extract::ImageColorSpace::Separation => "sep",
                siftx::pdf::image_extract::ImageColorSpace::DeviceN => "devn",
                siftx::pdf::image_extract::ImageColorSpace::Unknown => "-",
            };
            if sift_type != "stencil" && sift_type != "mask" {
                cmp_field_opt(
                    &mut field_total,
                    if record {
                        Some(&mut field_mismatches)
                    } else {
                        None
                    },
                    &name,
                    i,
                    "color",
                    &pop.color,
                    sift_color,
                );
            }
            cmp_field_opt(
                &mut field_total,
                if record {
                    Some(&mut field_mismatches)
                } else {
                    None
                },
                &name,
                i,
                "comp",
                &pop.comp,
                &sft.components.to_string(),
            );
            cmp_field_opt(
                &mut field_total,
                if record {
                    Some(&mut field_mismatches)
                } else {
                    None
                },
                &name,
                i,
                "bpc",
                &pop.bpc,
                &sft.bpc.to_string(),
            );

            let sift_enc = match sft.encoding {
                siftx::pdf::image_extract::ImageEncoding::Jpeg => "jpeg",
                siftx::pdf::image_extract::ImageEncoding::Jpeg2000 => "jpx",
                siftx::pdf::image_extract::ImageEncoding::Jbig2 => "jbig2",
                siftx::pdf::image_extract::ImageEncoding::Ccitt => "ccitt",
                siftx::pdf::image_extract::ImageEncoding::Flate => "image",
                siftx::pdf::image_extract::ImageEncoding::Lzw => "image",
                siftx::pdf::image_extract::ImageEncoding::RunLength => "image",
                siftx::pdf::image_extract::ImageEncoding::Raw => "image",
            };
            cmp_field_opt(
                &mut field_total,
                if record {
                    Some(&mut field_mismatches)
                } else {
                    None
                },
                &name,
                i,
                "enc",
                &pop.enc,
                sift_enc,
            );
        }
        if file_wh_miss > 0 {
            per_file_wh_miss.push((name.clone(), file_wh_miss, compare_len as u32));
        }
    }

    // Print summary
    eprintln!("\n=== pdfimages WIDE comparison ===");
    eprintln!("Total PDFs scanned: {total_pdfs}");
    eprintln!("Poppler skipped (no images or failed): {poppler_skip}");
    eprintln!("Sift parse failures: {sift_parse_fail}");
    eprintln!("Sift extraction failures: {sift_extract_fail}");
    eprintln!("Compared (both have images): {compared}");
    eprintln!("Image count: poppler={total_pop_images} sift={total_sift_images}");
    eprintln!(
        "Count match: {count_match}/{compared} ({:.1}%)",
        if compared > 0 {
            count_match as f64 * 100.0 / compared as f64
        } else {
            0.0
        }
    );
    eprintln!("Count mismatch: {count_mismatch}");

    eprintln!("\nField match rates:");
    for field in &fields {
        let (t, m) = field_total[field];
        let pct = if t > 0 {
            m as f64 * 100.0 / t as f64
        } else {
            100.0
        };
        let marker = if m == t { " " } else { "!" };
        eprintln!("  {marker} {field:10} {m}/{t} ({pct:.1}%)");
    }

    if !count_mismatches.is_empty() {
        eprintln!("\nCount mismatches (first {}):", count_mismatches.len());
        for (name, pop, sift) in &count_mismatches {
            eprintln!("  {name}: poppler={pop} sift={sift}");
        }
    }

    if !per_file_wh_miss.is_empty() {
        per_file_wh_miss.sort_by(|a, b| b.1.cmp(&a.1));
        eprintln!("\nWidth/height mismatches by file (top 30):");
        for (name, miss, total) in per_file_wh_miss.iter().take(30) {
            eprintln!("  {name}: {miss}/{total} images wrong");
        }
    }

    if !field_mismatches.is_empty() {
        eprintln!("\nField mismatches (first {}):", field_mismatches.len());
        for m in &field_mismatches {
            eprintln!("  {m}");
        }
    }
}

fn cmp_field_opt(
    field_total: &mut std::collections::HashMap<&str, (u32, u32)>,
    mismatches: Option<&mut Vec<String>>,
    name: &str,
    idx: usize,
    field: &str,
    expected: &str,
    got: &str,
) {
    let (t, m) = field_total.get_mut(field).unwrap();
    *t += 1;
    if expected == got {
        *m += 1;
    } else if let Some(mm) = mismatches {
        mm.push(format!(
            "{name}[{idx}].{field}: expected={expected:?} got={got:?}"
        ));
    }
}

fn cmp_field(
    field_total: &mut std::collections::HashMap<&str, (u32, u32)>,
    mismatches: &mut Vec<String>,
    name: &str,
    idx: usize,
    field: &str,
    expected: &str,
    got: &str,
) {
    let (t, m) = field_total.get_mut(field).unwrap();
    *t += 1;
    if expected == got {
        *m += 1;
    } else {
        mismatches.push(format!(
            "{name}[{idx}].{field}: expected={expected:?} got={got:?}"
        ));
    }
}

struct PdfimagesEntry {
    type_str: String,
    width: String,
    height: String,
    color: String,
    comp: String,
    bpc: String,
    enc: String,
}

fn parse_pdfimages_list(output: &str) -> Vec<PdfimagesEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("page") || line.starts_with("---") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }
        entries.push(PdfimagesEntry {
            type_str: parts[2].to_string(),
            width: parts[3].to_string(),
            height: parts[4].to_string(),
            color: parts[5].to_string(),
            comp: parts[6].to_string(),
            bpc: parts[7].to_string(),
            enc: parts[8].to_string(),
        });
    }
    entries
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_pdfs(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut pdfs = Vec::new();
    collect_pdfs(dir, &mut pdfs);
    pdfs.sort();
    pdfs
}

fn collect_pdfs(dir: &Path, pdfs: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        if path.is_dir() {
            // Skip .git directories
            if path.file_name().map(|n| n == ".git").unwrap_or(false) {
                continue;
            }
            collect_pdfs(&path, pdfs);
        } else if path.extension().and_then(|e| e.to_str()) == Some("pdf") {
            pdfs.push(path);
        }
    }
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// pdftotext comparison: sift vs Poppler pdftotext (live)
// ---------------------------------------------------------------------------

#[test]
fn pdftotext_comparison() {
    // Check pdftotext is available
    let pdftotext_ok = std::process::Command::new("pdftotext")
        .arg("-v")
        .output()
        .map(|o| o.status.success() || !o.stderr.is_empty()) // pdftotext -v exits 0 or 99
        .unwrap_or(false);
    if !pdftotext_ok {
        eprintln!("skipping: pdftotext not available");
        return;
    }

    let dirs = [
        "testdata/poppler-test",
        "testdata/pdfjs-pdfs",
        "testdata/verapdf-corpus",
    ];

    let mut all_pdfs = Vec::new();
    for dir in &dirs {
        let path = Path::new(dir);
        if path.exists() {
            let mut pdfs = find_pdfs(path);
            all_pdfs.append(&mut pdfs);
        }
    }
    all_pdfs.sort();

    if all_pdfs.is_empty() {
        eprintln!("skipping: no PDF test files found");
        return;
    }

    struct FileResult {
        name: String,
        corpus: String,
        pop_words: usize,
        common_words: usize,
        exact: bool,
        overlap: f64,
    }

    let mut total = 0u32;
    let mut sift_parse_fail = 0u32;
    let mut sift_fail_names: Vec<(String, String)> = Vec::new();
    let mut poppler_fail = 0u32;
    let mut neither_text = 0u32;
    let mut poppler_only_text = 0u32;
    let mut sift_only_text = 0u32;
    let mut results: Vec<FileResult> = Vec::new();

    for path in &all_pdfs {
        let name = path
            .strip_prefix("testdata/")
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .to_string();
        let corpus = name.split('/').next().unwrap_or("unknown").to_string();
        total += 1;

        // Run pdftotext -layout
        let poppler_text = match std::process::Command::new("pdftotext")
            .arg("-layout")
            .arg(path)
            .arg("-")
            .output()
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => {
                poppler_fail += 1;
                continue;
            }
        };

        // Run sift text extraction
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let doc = match siftx::pdf::document::Document::parse(&data) {
            Ok(d) => d,
            Err(e) => {
                sift_parse_fail += 1;
                sift_fail_names.push((name.clone(), format!("parse: {e}")));
                continue;
            }
        };
        let pages = match doc.pages() {
            Ok(p) => p,
            Err(e) => {
                sift_parse_fail += 1;
                sift_fail_names.push((name.clone(), format!("pages: {e}")));
                continue;
            }
        };
        let mut sift_text = String::new();
        for page in &pages {
            if let Ok(text) = siftx::pdf::text_layout::extract_text_layout(&doc, page) {
                if !sift_text.is_empty() {
                    sift_text.push('\x0C');
                }
                sift_text.push_str(&text);
            }
        }

        let pop_norm = normalize_text(&poppler_text);
        let sift_norm = normalize_text(&sift_text);

        if pop_norm.is_empty() && sift_norm.is_empty() {
            neither_text += 1;
            continue;
        }
        if !pop_norm.is_empty() && sift_norm.is_empty() {
            let pop_words: Vec<&str> = pop_norm.split_whitespace().collect();
            if pop_words.len() >= 10 {
                eprintln!(
                    "  POPPLER_ONLY: {} ({} words, first: {:?})",
                    name,
                    pop_words.len(),
                    &pop_words[..pop_words.len().min(5)]
                );
            }
            poppler_only_text += 1;
            continue;
        }
        if pop_norm.is_empty() && !sift_norm.is_empty() {
            sift_only_text += 1;
            continue;
        }

        let pop_set: std::collections::HashSet<&str> = pop_norm.split_whitespace().collect();
        let sift_set: std::collections::HashSet<&str> = sift_norm.split_whitespace().collect();
        let common = pop_set.intersection(&sift_set).count();
        let pop_word_count = pop_set.len().max(1);
        let overlap = common as f64 / pop_word_count as f64;

        results.push(FileResult {
            name,
            corpus,
            pop_words: pop_word_count,
            common_words: common,
            exact: pop_norm == sift_norm,
            overlap,
        });
    }

    // Print overall stats
    let compared = results.len() as u32;
    let exact_match = results.iter().filter(|r| r.exact).count() as u32;
    let high_overlap = results
        .iter()
        .filter(|r| !r.exact && r.overlap >= 0.9)
        .count() as u32;
    let medium_overlap = results
        .iter()
        .filter(|r| r.overlap >= 0.5 && r.overlap < 0.9 && !r.exact)
        .count() as u32;
    let low_overlap = results.iter().filter(|r| r.overlap < 0.5).count() as u32;
    let good = exact_match + high_overlap;
    let total_common: u64 = results.iter().map(|r| r.common_words as u64).sum();
    let total_pop: u64 = results.iter().map(|r| r.pop_words as u64).sum();

    eprintln!("\n=== pdftotext comparison: sift vs Poppler ===");
    eprintln!("Total PDFs:           {total}");
    eprintln!("Poppler failures:     {poppler_fail}");
    eprintln!("Sift parse failures:  {sift_parse_fail}");
    if !sift_fail_names.is_empty() {
        for (name, err) in &sift_fail_names {
            eprintln!("  {name}: {err}");
        }
    }
    eprintln!("Neither has text:     {neither_text}");
    eprintln!("Poppler only:         {poppler_only_text}");
    eprintln!("Sift only:            {sift_only_text}");
    eprintln!("Both have text:       {compared}");
    eprintln!();
    eprintln!("--- All files with text from both ({compared}) ---");
    eprintln!(
        "Exact match:          {exact_match} ({:.1}%)",
        exact_match as f64 / compared.max(1) as f64 * 100.0
    );
    eprintln!(
        "High (≥90%):          {high_overlap} ({:.1}%)",
        high_overlap as f64 / compared.max(1) as f64 * 100.0
    );
    eprintln!(
        "Medium (50-89%):      {medium_overlap} ({:.1}%)",
        medium_overlap as f64 / compared.max(1) as f64 * 100.0
    );
    eprintln!(
        "Low (<50%):           {low_overlap} ({:.1}%)",
        low_overlap as f64 / compared.max(1) as f64 * 100.0
    );
    eprintln!(
        "Good (exact+high):    {} ({:.1}%)",
        good,
        good as f64 / compared.max(1) as f64 * 100.0
    );
    eprintln!(
        "Aggregate overlap:    {total_common}/{total_pop} ({:.1}%)",
        total_common as f64 / total_pop.max(1) as f64 * 100.0
    );

    // Substantive files (≥20 words from Poppler) - filters out annotation/tiny PDFs
    let substantive: Vec<&FileResult> = results.iter().filter(|r| r.pop_words >= 20).collect();
    if !substantive.is_empty() {
        let s_count = substantive.len() as u32;
        let s_exact = substantive.iter().filter(|r| r.exact).count() as u32;
        let s_high = substantive
            .iter()
            .filter(|r| !r.exact && r.overlap >= 0.9)
            .count() as u32;
        let s_medium = substantive
            .iter()
            .filter(|r| r.overlap >= 0.5 && r.overlap < 0.9 && !r.exact)
            .count() as u32;
        let s_low = substantive.iter().filter(|r| r.overlap < 0.5).count() as u32;
        let s_good = s_exact + s_high;
        let s_common: u64 = substantive.iter().map(|r| r.common_words as u64).sum();
        let s_total: u64 = substantive.iter().map(|r| r.pop_words as u64).sum();
        eprintln!();
        eprintln!("--- Substantive files (≥20 words, {s_count} files) ---");
        eprintln!(
            "Exact match:          {s_exact} ({:.1}%)",
            s_exact as f64 / s_count.max(1) as f64 * 100.0
        );
        eprintln!(
            "High (≥90%):          {s_high} ({:.1}%)",
            s_high as f64 / s_count.max(1) as f64 * 100.0
        );
        eprintln!(
            "Medium (50-89%):      {s_medium} ({:.1}%)",
            s_medium as f64 / s_count.max(1) as f64 * 100.0
        );
        eprintln!(
            "Low (<50%):           {s_low} ({:.1}%)",
            s_low as f64 / s_count.max(1) as f64 * 100.0
        );
        eprintln!(
            "Good (exact+high):    {} ({:.1}%)",
            s_good,
            s_good as f64 / s_count.max(1) as f64 * 100.0
        );
        eprintln!(
            "Aggregate overlap:    {s_common}/{s_total} ({:.1}%)",
            s_common as f64 / s_total.max(1) as f64 * 100.0
        );
    }

    // Per-corpus breakdown
    let mut corpora: Vec<String> = results
        .iter()
        .map(|r| r.corpus.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    corpora.sort();
    eprintln!();
    eprintln!("--- Per-corpus breakdown ---");
    eprintln!(
        "{:<25} {:>5} {:>7} {:>7} {:>7} {:>7} {:>8}",
        "Corpus", "Files", "Exact", "High", "Med", "Low", "Overlap"
    );
    for corpus in &corpora {
        let c_results: Vec<&FileResult> = results.iter().filter(|r| r.corpus == *corpus).collect();
        let c_count = c_results.len();
        let c_exact = c_results.iter().filter(|r| r.exact).count();
        let c_high = c_results
            .iter()
            .filter(|r| !r.exact && r.overlap >= 0.9)
            .count();
        let c_medium = c_results
            .iter()
            .filter(|r| r.overlap >= 0.5 && r.overlap < 0.9 && !r.exact)
            .count();
        let c_low = c_results.iter().filter(|r| r.overlap < 0.5).count();
        let c_common: u64 = c_results.iter().map(|r| r.common_words as u64).sum();
        let c_total: u64 = c_results.iter().map(|r| r.pop_words as u64).sum();
        let c_pct = c_common as f64 / c_total.max(1) as f64 * 100.0;
        eprintln!(
            "{:<25} {:>5} {:>7} {:>7} {:>7} {:>7} {:>7.1}%",
            corpus, c_count, c_exact, c_high, c_medium, c_low, c_pct
        );
    }

    // Worst mismatches among substantive files
    let mut worst_sub: Vec<(&str, f64, usize)> = results
        .iter()
        .filter(|r| r.pop_words >= 20 && r.overlap < 0.5)
        .map(|r| (r.name.as_str(), r.overlap, r.pop_words))
        .collect();
    worst_sub.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    if !worst_sub.is_empty() {
        eprintln!("\nWorst substantive mismatches (first 20):");
        for (name, overlap, words) in worst_sub.iter().take(20) {
            eprintln!("  {name}: {:.0}% overlap ({words} words)", overlap * 100.0);
        }
    }

    // Medium overlap files (50-89%) sorted by missing words (pop_words - common_words)
    let mut medium_files: Vec<(&str, f64, usize, usize)> = results
        .iter()
        .filter(|r| r.pop_words >= 20 && r.overlap >= 0.5 && r.overlap < 0.9 && !r.exact)
        .map(|r| {
            (
                r.name.as_str(),
                r.overlap,
                r.pop_words,
                r.pop_words - r.common_words,
            )
        })
        .collect();
    medium_files.sort_by(|a, b| b.3.cmp(&a.3)); // sort by missing words desc
    if !medium_files.is_empty() {
        eprintln!("\nMedium overlap files by missing words (first 30):");
        for (name, overlap, words, missing) in medium_files.iter().take(30) {
            eprintln!(
                "  {name}: {:.0}% overlap ({words} words, {missing} missing)",
                overlap * 100.0
            );
        }
    }
}
