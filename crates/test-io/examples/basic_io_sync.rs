use std::fs::File;
use std::io::{BufReader, Read, Write};

/// Reader that always fails with a non-retryable error.
struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("boom"))
    }
}

/// Reader that always reports a retryable condition; must produce no
/// operation and no error in the report.
struct WouldBlockReader;

impl Read for WouldBlockReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
    }
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let data: Vec<u8> = (0u8..100).collect();
    let path = std::env::temp_dir().join("hotpath_basic_io_sync.bin");

    // Write the fixture file: 10 operations x 10 bytes plus one flush.
    let mut writer = hotpath::io!(File::create(&path).unwrap(), label = "fixture-write");
    for chunk in data.chunks(10) {
        writer.write_all(chunk).unwrap();
    }
    writer.flush().unwrap();
    drop(writer);

    // Read it back: 10 operations x 10 bytes, delegation verified byte-for-byte.
    let mut reader = hotpath::io!(File::open(&path).unwrap(), label = "fixture-read");
    let mut out = Vec::new();
    let mut buf = [0u8; 10];
    for _ in 0..10 {
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 10);
        out.extend_from_slice(&buf[..n]);
    }
    assert_eq!(out, data);

    // Wrapping a BufReader measures application-facing buffered reads instead
    // of actual file I/O.
    let mut buffered = hotpath::io!(
        BufReader::new(File::open(&path).unwrap()),
        label = "buffered"
    );
    let mut all = Vec::new();
    buffered.read_to_end(&mut all).unwrap();
    assert_eq!(all, data);

    // Non-retryable errors count; WouldBlock produces neither op nor error.
    let mut failing = hotpath::io!(FailingReader, label = "failing");
    assert!(failing.read(&mut buf).is_err());
    let mut busy = hotpath::io!(WouldBlockReader, label = "busy");
    assert!(busy.read(&mut buf).is_err());

    std::fs::remove_file(&path).ok();
    println!("Sync io example completed!");
}
