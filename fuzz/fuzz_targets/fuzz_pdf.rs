#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(doc) = siftx::pdf::document::Document::parse(data) {
        let _ = doc.info();
        let _ = doc.page_count();
        let _ = doc.pages();
        let _ = siftx::pdf::image_extract::extract_all_images(&doc);
    }
});
