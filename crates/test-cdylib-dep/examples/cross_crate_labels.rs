//! A plain binary that uses the literal label `"parse"`, which the
//! `test-cdylib-dep` library it links also uses.
//!
//! Run with:
//!   cargo run -p test-cdylib-dep --example cross_crate_labels --features hotpath

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::Json)
        .build();

    let local = hotpath::measure_block!("parse", std::hint::black_box(6));
    println!("total: {}", test_cdylib_dep::run() + local);
}
