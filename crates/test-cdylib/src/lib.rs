//! Built with:
//!   cargo build -p test-cdylib --lib --features hotpath
//!
//! A cdylib puts its dependencies' label symbols in the linker export list.

fn parse() -> u32 {
    hotpath::measure_block!("parse", std::hint::black_box(6))
}

#[no_mangle]
pub extern "C" fn test_cdylib_entry() -> u32 {
    test_cdylib_dep::run() + parse()
}
