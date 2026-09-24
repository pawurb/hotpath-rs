//! Build-failure fixture: `#[measure(label)]` and `measure_block!` share the
//! functions namespace, so one literal label on both is rejected. Expected to
//! fail codegen with
//! ``symbol `hotpath: duplicate function label "shared"` is already defined``.
//! The body is gated behind the `hotpath_dup_labels_fixture` cfg (not a
//! feature, so `--all-features` builds compile it to an empty program);
//! `tests/unique_labels.rs` enables it on purpose.
//!
//! Run with:
//!   cargo rustc -p test-all-features --example duplicate_labels_function --features hotpath -- --cfg hotpath_dup_labels_fixture

#[cfg(hotpath_dup_labels_fixture)]
#[hotpath::measure(label = "shared")]
fn measured() {}

#[cfg(hotpath_dup_labels_fixture)]
#[hotpath::main]
fn main() {
    measured();
    hotpath::measure_block!("shared", {
        std::hint::black_box(0);
    });
}

#[cfg(not(hotpath_dup_labels_fixture))]
fn main() {}
