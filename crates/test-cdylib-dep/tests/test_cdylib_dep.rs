//! Named after its package, so it shares the library's `CARGO_CRATE_NAME`;
//! reuses `"parse"`.
//!
//! Run with:
//!   cargo test -p test-cdylib-dep --features hotpath --test test_cdylib_dep

#[test]
fn test_same_label_as_library() {
    let local = hotpath::measure_block!("parse", std::hint::black_box(6));
    assert_eq!(test_cdylib_dep::run() + local, 21);
}
