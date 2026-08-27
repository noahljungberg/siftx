//! Integration tests: parse TIFF IFDs from real JPEG EXIF data.

use std::path::Path;

#[test]
fn parse_exif_tiff_from_jpegs() {
    let dir = Path::new("testdata/exiftool-images");
    if !dir.exists() {
        eprintln!("skipping: testdata not available");
        return;
    }

    let mut total = 0;
    let mut parsed = 0;
    let mut failures = Vec::new();

    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "jpg" && ext != "jpeg" {
            continue;
        }

        let data = std::fs::read(&path).unwrap();
        let segs = match siftx::jpeg::parse_segments(&data) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for seg in &segs {
            if let Some(tiff_data) = seg.exif_tiff_data() {
                total += 1;
                match siftx::tiff::parse_tiff(tiff_data) {
                    Ok((header, ifds)) => {
                        parsed += 1;
                        // Basic sanity: should have at least IFD0
                        assert!(!ifds.is_empty(), "no IFDs in {name}");
                        // IFD0 should have entries
                        assert!(!ifds[0].entries.is_empty(), "empty IFD0 in {name}");
                        let _ = header; // used
                    }
                    Err(e) => {
                        failures.push(format!("{name}: {e}"));
                    }
                }
            }
        }
    }

    eprintln!("Parsed {parsed}/{total} EXIF TIFF structures from JPEGs");

    if !failures.is_empty() {
        panic!(
            "Failed to parse {} EXIF blocks:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    assert!(total > 0, "no EXIF data found in test corpus");
}

#[test]
fn parse_tiff_file() {
    let path = Path::new("testdata/exiftool-images/ExifTool.tif");
    if !path.exists() {
        eprintln!("skipping: ExifTool.tif not available");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let (header, ifds) = siftx::tiff::parse_tiff(&data).unwrap();
    eprintln!(
        "ExifTool.tif: big_endian={}, bigtiff={}, {} IFDs",
        header.big_endian,
        header.bigtiff,
        ifds.len()
    );
    assert!(!ifds.is_empty());

    // Print IFD0 tags for verification
    for entry in &ifds[0].entries {
        eprintln!(
            "  tag={} type={:?} count={}",
            entry.tag, entry.data_type, entry.count
        );
    }
}

#[test]
fn multipage_tiff_scan() {
    use siftx::tiff::exif::TiffDocument;

    let tiff_dirs = [
        "testdata/fuzzing-seeds/tiff/go-fuzz",
        "testdata/fuzzing-seeds/tiff/mopt",
        "testdata/exiftool-images",
    ];

    // Unlike the single-corpus tests, this one walks several directories and
    // skips the ones that are absent - so it needs its own guard, or the
    // "found something" assertion at the end fires on a checkout with no
    // corpora at all. That is exactly what happened in CI.
    if !tiff_dirs.iter().any(|d| Path::new(d).exists()) {
        eprintln!("skipping: testdata not available");
        return;
    }

    let mut total = 0;
    let mut multi_page = 0;
    let mut with_sub_ifds = 0;
    let mut max_pages = 0usize;

    for dir_path in &tiff_dirs {
        let dir = Path::new(dir_path);
        if !dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            // Parse standalone TIFF files
            if ext == "tif" || ext == "tiff" {
                let data = match std::fs::read(&path) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if let Ok(doc) = TiffDocument::parse(&data) {
                    total += 1;
                    max_pages = max_pages.max(doc.page_count());
                    if doc.page_count() > 1 {
                        multi_page += 1;
                    }
                    for page in &doc.pages {
                        if !page.sub_ifds.is_empty() {
                            with_sub_ifds += 1;
                        }
                    }
                }
            }

            // Also parse TIFF from JPEG EXIF
            if ext == "jpg" || ext == "jpeg" {
                let data = match std::fs::read(&path) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if let Ok(segs) = siftx::jpeg::parse_segments(&data) {
                    for seg in &segs {
                        if let Some(tiff_data) = seg.exif_tiff_data() {
                            if let Ok(doc) = TiffDocument::parse(tiff_data) {
                                total += 1;
                                max_pages = max_pages.max(doc.page_count());
                                if doc.page_count() > 1 {
                                    multi_page += 1;
                                }
                                for page in &doc.pages {
                                    if !page.sub_ifds.is_empty() {
                                        with_sub_ifds += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    eprintln!("\nTiffDocument scan:");
    eprintln!("  Total parsed: {total}");
    eprintln!("  Multi-page: {multi_page}");
    eprintln!("  Pages with SubIFDs: {with_sub_ifds}");
    eprintln!("  Max pages in one file: {max_pages}");

    assert!(total > 0, "no TIFF data found in test corpus");
}
