//! Integration tests: parse real JPEG files from the test corpus.

use std::path::Path;

#[test]
fn parse_all_test_jpegs() {
    let dir = Path::new("testdata/exiftool-images");
    if !dir.exists() {
        eprintln!("skipping: testdata not available");
        return;
    }

    let mut total = 0;
    let mut with_exif = 0;
    let mut with_xmp = 0;
    let mut with_icc = 0;
    let mut with_iptc = 0;
    let mut failures = Vec::new();

    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        // Only test JPEG files
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "jpg" && ext != "jpeg" {
            continue;
        }

        total += 1;
        let data = std::fs::read(&path).unwrap();
        match siftx::jpeg::parse_segments(&data) {
            Ok(segs) => {
                // Should always have at least SOI
                assert!(!segs.is_empty(), "no segments in {name}");
                assert_eq!(
                    segs[0].marker,
                    siftx::jpeg::Marker::Soi,
                    "first segment not SOI in {name}"
                );

                for seg in &segs {
                    if seg.app1_kind() == Some(siftx::jpeg::App1Kind::Exif) {
                        with_exif += 1;
                    }
                    if seg.app1_kind() == Some(siftx::jpeg::App1Kind::Xmp) {
                        with_xmp += 1;
                    }
                    if seg.is_icc_profile() {
                        with_icc += 1;
                    }
                    if seg.is_photoshop() {
                        with_iptc += 1;
                    }
                }
            }
            Err(e) => {
                failures.push(format!("{name}: {e}"));
            }
        }
    }

    eprintln!(
        "Parsed {total} JPEGs: {with_exif} EXIF, {with_xmp} XMP, {with_icc} ICC, {with_iptc} IPTC"
    );

    if !failures.is_empty() {
        panic!(
            "Failed to parse {} files:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    assert!(total > 0, "no JPEG files found in testdata");
}
