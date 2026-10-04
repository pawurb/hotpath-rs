//! Built with:
//!   cargo build -p test-cdylib --lib --features hotpath
//!
//! A cdylib exports every `#[no_mangle]`/`#[export_name]` symbol of its
//! dependencies, so the literal labels of `test-cdylib-dep` end up in the
//! linker export list. `"parse"` repeats a label `test-cdylib-dep` also uses.

fn parse() -> u32 {
    hotpath::measure_block!("parse", std::hint::black_box(6))
}

#[no_mangle]
pub extern "C" fn test_cdylib_entry() -> u32 {
    test_cdylib_dep::run() + parse()
}
