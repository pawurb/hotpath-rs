//! An integration test named after its own package, so it compiles under the
//! library's `CARGO_CRATE_NAME` without `CARGO_BIN_NAME`, and repeats the
//! library's `"parse"` label.
//!
//! Run with:
//!   cargo test -p test-cdylib-dep --features hotpath --test test_cdylib_dep

#[test]
fn test_same_label_as_library() {
    let local = hotpath::measure_block!("parse", std::hint::black_box(6));
    assert_eq!(test_cdylib_dep::run() + local, 21);
}
