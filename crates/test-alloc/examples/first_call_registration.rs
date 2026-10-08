//! `outer` allocates nothing but first-calls measured callees, whose
//! location registration grows the registry inside its scope. It must
//! still report 0 B.
//!
//! Run with:
//!   cargo run -p test-alloc --example first_call_registration --features hotpath,hotpath-alloc

#[hotpath::measure]
fn callee_a(x: u64) -> u64 {
    std::hint::black_box(x + 1)
}

#[hotpath::measure]
fn callee_b(x: u64) -> u64 {
    std::hint::black_box(x + 2)
}

#[hotpath::measure]
fn callee_c(x: u64) -> u64 {
    std::hint::black_box(x + 3)
}

#[hotpath::measure]
fn callee_d(x: u64) -> u64 {
    std::hint::black_box(x + 4)
}

#[hotpath::measure]
fn outer(x: u64) -> u64 {
    callee_d(callee_c(callee_b(callee_a(x))))
}

#[hotpath::main]
fn main() {
    let mut acc = 0;
    for _ in 0..10 {
        acc = outer(acc);
    }
    std::hint::black_box(acc);
}
