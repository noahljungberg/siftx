use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::fs;

fn bench_jpeg(c: &mut Criterion) {
    let paths = sample_files(
        "jpg",
        &["testdata/exiftool-images", "testdata/exif-samples"],
    );
    let mut group = c.benchmark_group("jpeg");
    for path in &paths {
        let data = fs::read(path).unwrap();
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        group.bench_with_input(
            BenchmarkId::new("parse_segments", name),
            &data,
            |b, data| b.iter(|| siftx::jpeg::parse_segments(data)),
        );
    }
    group.finish();
}

fn bench_tiff(c: &mut Criterion) {
    let paths = sample_files("tif", &["testdata/exiftool-images"]);
    let mut group = c.benchmark_group("tiff");
    for path in &paths {
        let data = fs::read(path).unwrap();
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        group.bench_with_input(BenchmarkId::new("parse_tiff", name), &data, |b, data| {
            b.iter(|| siftx::tiff::parse_tiff(data))
        });
    }
    group.finish();
}

fn bench_png(c: &mut Criterion) {
    let paths = sample_files(
        "png",
        &["testdata/exiftool-images", "testdata/exif-samples"],
    );
    let mut group = c.benchmark_group("png");
    for path in &paths {
        let data = fs::read(path).unwrap();
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        group.bench_with_input(BenchmarkId::new("parse_chunks", name), &data, |b, data| {
            b.iter(|| siftx::png::parse_chunks(data))
        });
    }
    group.finish();
}

fn bench_pdf(c: &mut Criterion) {
    let paths = sample_files("pdf", &["testdata/pdfjs-pdfs", "testdata/poppler-test"]);
    let mut group = c.benchmark_group("pdf");
    group.sample_size(20);
    for path in &paths {
        let data = fs::read(path).unwrap();
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        group.bench_with_input(
            BenchmarkId::new("document_parse", name),
            &data,
            |b, data| b.iter(|| siftx::pdf::document::Document::parse(data)),
        );
    }
    group.finish();
}

fn bench_read_auto(c: &mut Criterion) {
    // Benchmark the high-level auto-detect + parse + tags pipeline
    let mut all: Vec<String> = Vec::new();
    for ext in &["jpg", "png", "tif", "webp", "gif", "bmp"] {
        all.extend(sample_files(ext, &["testdata/exiftool-images"]));
    }
    all.extend(sample_files("pdf", &["testdata/pdfjs-pdfs"]));

    let mut group = c.benchmark_group("read_auto");
    group.sample_size(20);
    for path in &all {
        let data = fs::read(path).unwrap();
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        group.bench_with_input(BenchmarkId::new("read+tags", name), &data, |b, data| {
            b.iter(|| {
                if let Ok(doc) = siftx::read(data) {
                    let _ = doc.tags();
                }
            })
        });
    }
    group.finish();
}

/// Collect up to 5 sample files with the given extension from the given directories.
fn sample_files(ext: &str, dirs: &[&str]) -> Vec<String> {
    let mut files = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case(ext))
                .unwrap_or(false)
            {
                files.push(path.to_string_lossy().into_owned());
                if files.len() >= 5 {
                    return files;
                }
            }
        }
    }
    files
}

criterion_group!(
    benches,
    bench_jpeg,
    bench_tiff,
    bench_png,
    bench_pdf,
    bench_read_auto
);
criterion_main!(benches);
