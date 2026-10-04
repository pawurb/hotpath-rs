//! Literal labels that are not valid linker symbols as-is, plus `"parse"`,
//! which dependents of this crate reuse.

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
