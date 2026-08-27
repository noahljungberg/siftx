#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(chunks) = siftx::png::parse_chunks(data) {
        let _ = siftx::png::find_exif_chunk(&chunks);
        let _ = siftx::png::find_iccp_chunk(&chunks);
        for chunk in &chunks {
            match &chunk.chunk_type {
                b"tEXt" => { let _ = siftx::png::parse_text(chunk); }
                b"iTXt" => { let _ = siftx::png::parse_itxt(chunk); }
                b"zTXt" => { let _ = siftx::png::parse_ztxt(chunk); }
                b"IHDR" => { let _ = siftx::png::parse_ihdr(chunk); }
                b"pHYs" => { let _ = siftx::png::parse_phys(chunk); }
                _ => {}
            }
        }
    }
});
