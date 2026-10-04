//! Same package and `CARGO_CRATE_NAME` as the library, reusing `"parse"`.
//!
//! Run with:
//!   cargo run -p test-cdylib-dep --bin test-cdylib-dep --features hotpath

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::Json)
        .build();

    let local = hotpath::measure_block!("parse", std::hint::black_box(6));
    println!("total: {}", test_cdylib_dep::run() + local);
}
