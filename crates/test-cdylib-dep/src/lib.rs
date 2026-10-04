//! Literal labels with characters that are not valid in a linker symbol list
//! or longer than its length cap,
//! plus `"parse"`, which `test-cdylib` reuses to cover the same label in two
//! crates of one dependency graph.

#[hotpath::measure(label = "with space")]
pub fn spaced() -> u32 {
    std::hint::black_box(1)
}

#[hotpath::measure(label = "quote\"d")]
pub fn quoted() -> u32 {
    std::hint::black_box(2)
}

#[hotpath::measure(label = "unicode ünï")]
pub fn unicode() -> u32 {
    std::hint::black_box(3)
}

#[hotpath::measure(label = "mod::path")]
pub fn path() -> u32 {
    std::hint::black_box(4)
}

// Longer than the symbol's label cap, so it is cut and hashed.
#[hotpath::measure(
    label = "a_label_long_enough_to_exceed_the_symbol_length_cap_of_hotpath_unique_labels"
)]
pub fn long() -> u32 {
    std::hint::black_box(0)
}

pub fn parse() -> u32 {
    hotpath::measure_block!("parse", std::hint::black_box(5))
}

pub fn run() -> u32 {
    spaced() + quoted() + unicode() + path() + long() + parse()
}
