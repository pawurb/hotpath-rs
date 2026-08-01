//! Exercises `io!` across several stream kinds in one run - an in-memory
//! buffer baseline, file writes and reads (separate wrappers), an in-process
//! TCP echo server, an optional
//! remote TCP endpoint, and gzip and brotli codecs - producing a multi-row
//! I/O report that contrasts bytes/s rates across I/O types. The remote
//! section reads `REMOTE_TCP_ADDR` (`host:port`, expects an echo server) and
//! is skipped when the variable is unset or the target is unreachable.
//!
//! Run with:
//!   cargo run -p test-io --example multi_io --features hotpath

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

const CHUNK_SIZES: &[usize] = &[256 * 1024, 512 * 1024, 1024 * 1024, 4 * 1024 * 1024];
const MAX_CHUNK: usize = 4 * 1024 * 1024;
const FILE_TOTAL: usize = 64 * 1024 * 1024;
const MEM_TOTAL: usize = 64 * 1024 * 1024;
const COMPRESS_TOTAL: usize = 16 * 1024 * 1024;

// Echo round trips are lockstep (write chunk, read it back), so chunks must
// stay below the kernel socket buffers to avoid a write/write deadlock.
const TCP_CHUNK_SIZES: &[usize] = &[4 * 1024, 16 * 1024, 64 * 1024];
const TCP_MAX_CHUNK: usize = 64 * 1024;
const TCP_TOTAL: usize = 32 * 1024 * 1024;

const REMOTE_TOTAL: usize = 1024 * 1024;

// Writes `total` bytes in cycling chunk sizes, so per-operation durations
// spread instead of collapsing into identical Avg and P95.
fn write_chunks(writer: &mut impl Write, chunk: &[u8], total: usize) {
    let (mut written, mut i) = (0, 0);
    while written < total {
        let size = CHUNK_SIZES[i % CHUNK_SIZES.len()];
        writer.write_all(&chunk[..size]).unwrap();
        written += size;
        i += 1;
    }
}

fn read_chunks(reader: &mut impl Read) {
    let mut buf = vec![0u8; MAX_CHUNK];
    for i in 0.. {
        let size = CHUNK_SIZES[i % CHUNK_SIZES.len()];
        if reader.read(&mut buf[..size]).unwrap() == 0 {
            break;
        }
        // Keeps the copied bytes observable; without this, release builds
        // eliminate the Cursor memcpy and the memory row reads in ~10 ns.
        std::hint::black_box(&buf);
    }
}

// Pushes file I/O to the physical disk instead of the page cache. macOS
// F_NOCACHE makes subsequent I/O on this descriptor bypass the cache; Linux
// POSIX_FADV_DONTNEED drops the file's already-cached pages, so call it on
// the read descriptor after the writer has synced.
#[cfg(target_os = "macos")]
fn bypass_page_cache(file: &std::fs::File) {
    use std::os::fd::AsRawFd;
    unsafe {
        libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1);
    }
}

#[cfg(target_os = "linux")]
fn bypass_page_cache(file: &std::fs::File) {
    use std::os::fd::AsRawFd;
    unsafe {
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn bypass_page_cache(_file: &std::fs::File) {}

// Returns the file contents so the memory section can replay the exact same
// bytes from RAM, contrasting real disk I/O with plain memcpys.
fn file_section(chunk: &[u8]) -> Vec<u8> {
    let path = std::env::temp_dir().join("hotpath_multi_io.bin");

    let writer = std::fs::File::create(&path).unwrap();
    bypass_page_cache(&writer);
    let mut file = hotpath::io!(writer, label = "file-write");
    write_chunks(&mut file, chunk, FILE_TOTAL);
    hotpath::io_unwrap(file).sync_all().unwrap();

    let reader = std::fs::File::open(&path).unwrap();
    bypass_page_cache(&reader);
    let mut file = hotpath::io!(reader, label = "file-read");
    read_chunks(&mut file);
    drop(file);

    let contents = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    contents
}

// In-memory baseline over the same bytes the file section pushed through
// syscalls: writes are Vec memcpys (plus the occasional realloc), reads are
// slice copies out of a Cursor. The gap between the file-* and memory rows is
// the syscall and physical disk cost.
fn memory_section(file_bytes: &[u8]) {
    let mut sink = hotpath::io!(Vec::new(), label = "memory");
    write_chunks(&mut sink, &file_bytes[..MAX_CHUNK], MEM_TOTAL);
    drop(sink);

    let mut source = hotpath::io!(std::io::Cursor::new(file_bytes), label = "memory");
    read_chunks(&mut source);
}

fn local_tcp_section(chunk: &[u8]) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = vec![0u8; TCP_MAX_CHUNK];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).unwrap();
        }
    });

    let stream = TcpStream::connect(addr).unwrap();
    stream.set_nodelay(true).unwrap();
    let mut tcp = hotpath::io!(stream, label = "tcp-local");

    let mut echo = vec![0u8; TCP_MAX_CHUNK];
    let (mut sent, mut i) = (0, 0);
    while sent < TCP_TOTAL {
        let size = TCP_CHUNK_SIZES[i % TCP_CHUNK_SIZES.len()];
        tcp.write_all(&chunk[..size]).unwrap();
        tcp.read_exact(&mut echo[..size]).unwrap();
        assert_eq!(&echo[..size], &chunk[..size]);
        sent += size;
        i += 1;
    }

    drop(tcp);
    server.join().unwrap();
}

fn remote_tcp_section(chunk: &[u8]) {
    let Ok(addr) = std::env::var("REMOTE_TCP_ADDR") else {
        eprintln!("skipping remote TCP section: REMOTE_TCP_ADDR not set (expected host:port)");
        return;
    };
    let Ok(sock) = TcpStream::connect(&addr) else {
        eprintln!("skipping remote TCP section: cannot reach {addr}");
        return;
    };
    sock.set_nodelay(true).unwrap();
    let mut remote = hotpath::io!(sock, label = "tcp-remote");

    let mut echo = vec![0u8; TCP_MAX_CHUNK];
    let (mut sent, mut i) = (0, 0);
    while sent < REMOTE_TOTAL {
        let size = TCP_CHUNK_SIZES[i % TCP_CHUNK_SIZES.len()];
        remote.write_all(&chunk[..size]).unwrap();
        remote.read_exact(&mut echo[..size]).unwrap();
        assert_eq!(&echo[..size], &chunk[..size]);
        sent += size;
        i += 1;
    }
}

fn gzip_section(chunk: &[u8]) {
    let mut encoder = hotpath::io!(
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()),
        label = "gzip"
    );
    write_chunks(&mut encoder, chunk, COMPRESS_TOTAL);
    // finish(self) consumes the encoder, so peel the wrapper off with io_unwrap.
    let compressed = hotpath::io_unwrap(encoder).finish().unwrap();

    let mut decoder = hotpath::io!(
        flate2::read::GzDecoder::new(&compressed[..]),
        label = "gzip"
    );
    read_chunks(&mut decoder);
}

fn brotli_section(chunk: &[u8]) {
    let mut encoder = hotpath::io!(
        brotli::CompressorWriter::new(Vec::new(), 4096, 5, 22),
        label = "brotli"
    );
    write_chunks(&mut encoder, chunk, COMPRESS_TOTAL);
    // into_inner finalizes the brotli stream before returning the buffer.
    let compressed = hotpath::io_unwrap(encoder).into_inner();

    let mut decoder = hotpath::io!(
        brotli::Decompressor::new(&compressed[..], 4096),
        label = "brotli"
    );
    read_chunks(&mut decoder);
}

// Low-entropy data (16 symbols), so the codecs compress and decompress it at
// realistic text-like speeds instead of degenerating into stored-block copies.
fn pseudo_random_chunk(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            b'a' + (state as u8 & 0x0F)
        })
        .collect()
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let chunk = pseudo_random_chunk(MAX_CHUNK);
    let file_bytes = file_section(&chunk);
    memory_section(&file_bytes);
    local_tcp_section(&chunk);
    remote_tcp_section(&chunk);
    gzip_section(&chunk);
    brotli_section(&chunk);

    println!("multi_io example completed!");
}
