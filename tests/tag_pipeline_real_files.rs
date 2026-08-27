//! Integration tests: validate tag table and value pipeline against real files.

use siftx::tiff::tags::{self, TagGroup};
use siftx::tiff::value::TagValue;
use std::path::Path;

#[test]
fn tag_pipeline_on_real_jpegs() {
    let dir = Path::new("testdata/exiftool-images");
    if !dir.exists() {
        eprintln!("skipping: testdata not available");
        return;
    }

    let mut total_files = 0;
    let mut total_tags = 0;
    let mut known_tags = 0;
    let mut with_print_conv = 0;

    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
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
                let exif = match siftx::tiff::exif::ExifData::parse(tiff_data) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                total_files += 1;
                let be = exif.header.big_endian;

                // Process IFD0 tags
                for entry in &exif.ifd0.entries {
                    total_tags += 1;
                    if let Some(tag_def) = tags::find_tag(entry.tag, TagGroup::Ifd0) {
                        known_tags += 1;
                        if let Some(val) = TagValue::from_entry(entry, be) {
                            let _display = tags::print_value(tag_def, &val);
                            if tag_def.print_conv.is_some() {
                                with_print_conv += 1;
                            }
                        }
                    }
                }

                // Process ExifIFD tags
                if let Some(exif_ifd) = &exif.exif_ifd {
                    for entry in &exif_ifd.entries {
                        total_tags += 1;
                        if let Some(tag_def) = tags::find_tag(entry.tag, TagGroup::ExifIfd) {
                            known_tags += 1;
                            if let Some(val) = TagValue::from_entry(entry, be) {
                                let _display = tags::print_value(tag_def, &val);
                                if tag_def.print_conv.is_some() {
                                    with_print_conv += 1;
                                }
                            }
                        }
                    }
                }

                // Process GPS tags
                if let Some(gps_ifd) = &exif.gps_ifd {
                    for entry in &gps_ifd.entries {
                        total_tags += 1;
                        if let Some(tag_def) = tags::find_tag(entry.tag, TagGroup::GpsIfd) {
                            known_tags += 1;
                            if let Some(val) = TagValue::from_entry(entry, be) {
                                let _display = tags::print_value(tag_def, &val);
                            }
                        }
                    }
                }
            }
        }
    }

    eprintln!(
        "Tag pipeline: {total_files} files, {total_tags} tags total, \
         {known_tags} known ({:.0}%), {with_print_conv} with PrintConv",
        known_tags as f64 / total_tags as f64 * 100.0
    );

    assert!(total_files > 0, "no EXIF data found");
    assert!(known_tags > 0, "no known tags found");
    // Most tags should be recognized
    assert!(
        known_tags as f64 / total_tags as f64 > 0.5,
        "fewer than 50% of tags recognized: {known_tags}/{total_tags}"
    );
}
