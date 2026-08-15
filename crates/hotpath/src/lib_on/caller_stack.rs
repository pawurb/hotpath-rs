//! Thread-local stack of instrumented-function names used to attribute SQL
//! queries and HTTP requests to the innermost measured function (the "Source"
//! column). Sync measurement guards push on creation and pop on drop; async
//! bodies push and pop around every poll (see `futures::wrapper`), so tasks
//! interleaved on one runtime thread never observe a stale caller.
//!
//! Compiles to no-ops unless a SQL or HTTP front-end feature is enabled.

cfg_if::cfg_if! {
    if #[cfg(any(
        feature = "sqlx",
        feature = "diesel",
        feature = "toasty",
        feature = "reqwest-0-12",
        feature = "reqwest-0-13",
        feature = "ureq-3",
    ))] {
        use std::cell::Cell;

        const MAX_DEPTH: usize = 64;

        struct CallerStack {
            depth: Cell<usize>,
            names: [Cell<&'static str>; MAX_DEPTH],
        }

        thread_local! {
            static CALLER_STACK: CallerStack = const {
                CallerStack {
                    depth: Cell::new(0),
                    names: [const { Cell::new("") }; MAX_DEPTH],
                }
            };
        }

        /// Pushes beyond `MAX_DEPTH` only bump the depth counter so pops stay
        /// balanced; `current_caller` then reports the deepest recorded name.
        #[inline]
        pub(crate) fn push_caller(name: &'static str) {
            let _ = CALLER_STACK.try_with(|stack| {
                let depth = stack.depth.get();
                if depth < MAX_DEPTH {
                    stack.names[depth].set(name);
                }
                stack.depth.set(depth + 1);
            });
        }

        #[inline]
        pub(crate) fn pop_caller() {
            let _ = CALLER_STACK.try_with(|stack| {
                let depth = stack.depth.get();
                debug_assert!(depth > 0, "pop_caller called with depth 0");
                if depth > 0 {
                    stack.depth.set(depth - 1);
                }
            });
        }

        #[inline]
        pub(crate) fn current_caller() -> Option<&'static str> {
            CALLER_STACK
                .try_with(|stack| {
                    let depth = stack.depth.get();
                    if depth == 0 {
                        None
                    } else {
                        Some(stack.names[depth.min(MAX_DEPTH) - 1].get())
                    }
                })
                .ok()
                .flatten()
        }
    } else {
        #[inline]
        pub(crate) fn push_caller(_name: &'static str) {}

        #[inline]
        pub(crate) fn pop_caller() {}

        #[inline]
        #[allow(dead_code)]
        pub(crate) fn current_caller() -> Option<&'static str> {
            None
        }
    }
}
