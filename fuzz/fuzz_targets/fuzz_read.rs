#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Auto-detect format and parse - exercises all format parsers
    if let Ok(doc) = siftx::read(data) {
        let _ = doc.tags();
        let _ = doc.gps();
        let _ = doc.text_pages();
        let _ = doc.images();
    }
});
