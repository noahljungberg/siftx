//! One group must not hold two tags with the same name.
//!
//! A group is a namespace. The same NAME in two groups is information - a PDF
//! states CreateDate in both its Info dictionary and its XMP, in different
//! formats, and a reader wants to see both. The same name TWICE IN ONE GROUP is
//! the problem: a table then shows
//!
//!     Saturation   Normal
//!     Saturation   0
//!
//! with no way to tell which is which, and it reads as corruption rather than
//! as metadata. That is exactly how it looked when a consumer first rendered
//! raw tags.
//!
//! The usual cause is ours, not the file's: a maker note carries a field both
//! as a top-level IFD tag and inside a binary sub-block, and both decoders emit
//! the plain name. The fix is at the source - name the sub-block's field for
//! its block, the way `decode_nikon_picture_control` now does.
//!
//! KNOWN below is a ratchet, not a target. Everything in it is measured, with
//! the reason it is there; a collision that is NOT in it fails this test. Fix
//! new ones at the source rather than adding a line here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Collisions that exist today, with why. Measured 2026-08-16 over the whole
/// corpus (exif-samples, exiftool-images, pdfjs-pdfs, format-corpus).
///
/// Two kinds:
///   FILE  - the file genuinely says it twice (repeated or conflicting IFDs).
///           Showing both is correct; there is nothing to fix in sift.
///   OURS  - a sub-block decoder reusing a top-level name. Same class as the
///           Nikon PictureControl fix; each needs its own naming decision.
const KNOWN: &[(&str, &str, &str)] = &[
    // 87_OSError.jpg carries a second, conflicting EXIF IFD - every one of
    // these is that one file saying it twice. FILE.
    (
        "EXIF",
        "WhiteBalance",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    (
        "EXIF",
        "Saturation",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    ("EXIF", "Contrast", "FILE: duplicated IFD in 87_OSError.jpg"),
    (
        "EXIF",
        "Sharpness",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    (
        "EXIF",
        "ExposureMode",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    (
        "EXIF",
        "GainControl",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    (
        "EXIF",
        "DigitalZoomRatio",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    (
        "EXIF",
        "SceneCaptureType",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    (
        "EXIF",
        "SubjectDistanceRange",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    (
        "EXIF",
        "FocalLengthIn35mmFormat",
        "FILE: duplicated IFD in 87_OSError.jpg",
    ),
    // Padding is a real per-IFD tag: a file with several IFDs has several. FILE.
    ("EXIF", "Padding", "FILE: one Padding tag per IFD"),
    // Maker-note sub-blocks reusing a top-level name. OURS.
    (
        "MakerNotes",
        "LensFStops",
        "OURS: Nikon LensData vs the IFD tag (4 files)",
    ),
    (
        "MakerNotes",
        "ShutterCount",
        "OURS: Nikon LensData/ShotInfo vs the IFD tag",
    ),
    (
        "MakerNotes",
        "FocalPlaneDiagonal",
        "OURS: Olympus equipment block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "ColorMatrix",
        "OURS: Olympus raw-development block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "CoringFilter",
        "OURS: Olympus raw-development block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "ValidBits",
        "OURS: Olympus raw-development block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "FirmwareVersion",
        "OURS: Canon camera-info block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "LensType",
        "OURS: Canon lens-info block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "FlashOutput",
        "OURS: Canon camera-info block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "CameraTemperature",
        "OURS: Canon camera-info block vs the IFD tag",
    ),
    (
        "MakerNotes",
        "PentaxModelID",
        "OURS: Pentax camera-info block vs the IFD tag",
    ),
];

fn walk(dir: &Path, found: &mut BTreeMap<(String, String), Vec<String>>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "tif" | "tiff" | "png" | "webp" | "heic" | "pdf" | "mov" | "mp4"
        ) {
            continue;
        }
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        // A file we cannot parse has no tags to collide - not this test's problem.
        let Ok(doc) = siftx::read(&data) else {
            continue;
        };
        let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
        for t in doc.tags() {
            *seen
                .entry((t.group.to_string(), t.name.clone()))
                .or_default() += 1;
        }
        for (key, count) in seen {
            if count > 1 {
                found.entry(key).or_default().push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
}

#[test]
fn no_group_holds_two_tags_with_the_same_name() {
    let dirs = [
        "testdata/exif-samples",
        "testdata/exiftool-images",
        "testdata/pdfjs-pdfs",
        "testdata/format-corpus",
    ];
    let mut found = BTreeMap::new();
    let mut scanned = 0;
    for d in dirs {
        let p = Path::new(d);
        if p.exists() {
            scanned += 1;
            walk(p, &mut found);
        }
    }
    if scanned == 0 {
        eprintln!("skipping: testdata not available");
        return;
    }

    let known: BTreeSet<(&str, &str)> = KNOWN.iter().map(|(g, n, _)| (*g, *n)).collect();
    let new: Vec<String> = found
        .iter()
        .filter(|((g, n), _)| !known.contains(&(g.as_str(), n.as_str())))
        .map(|((g, n), files)| format!("  {g}/{n} - {} file(s), e.g. {}", files.len(), files[0]))
        .collect();

    assert!(
        new.is_empty(),
        "a group now holds two tags with the same name, which renders as \
         corruption in a metadata table:\n{}\n\nName the sub-block's field for \
         its block (see decode_nikon_picture_control) rather than adding it to \
         KNOWN - read this file's header first.",
        new.join("\n"),
    );

    // The ratchet only tightens: a KNOWN entry that stopped colliding means
    // someone fixed it, and leaving the line behind lets the next one hide.
    let stale: Vec<&str> = KNOWN
        .iter()
        .filter(|(g, n, _)| !found.contains_key(&((*g).to_string(), (*n).to_string())))
        .map(|(_, n, _)| *n)
        .collect();
    assert!(
        stale.is_empty(),
        "these no longer collide - delete them from KNOWN: {stale:?}",
    );
}
