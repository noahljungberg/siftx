#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(segments) = siftx::jpeg::parse_segments(data) {
        let _ = siftx::jpeg::reassemble_icc_profile(&segments);
        let _ = siftx::jpeg::reassemble_extended_xmp(&segments);
    }
});
