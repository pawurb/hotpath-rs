//! Thread-local stack of instrumented-function names used to attribute SQL
//! queries and HTTP requests to the innermost measured function (the "Source"
//! column). Sync measurement guards push on creation and pop on drop; async
//! bodies push and pop around every poll (see `futures::wrapper`), so tasks
//! interleaved on one runtime thread never observe a stale caller.
//!
//! Compiles to no-ops unless a SQL or HTTP front-end feature is enabled.
//!
//! Also holds the per-thread axum route context (the "Route" column): the
//! server middleware enters the matched route template around every poll of
//! the handler future, and SQL/HTTP front-ends read it alongside the caller.

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

cfg_if::cfg_if! {
    if #[cfg(feature = "axum-0-8")] {
        use std::collections::HashSet;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::RwLock;

        thread_local! {
            static CURRENT_ROUTE: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
        }

        static ROUTE_SCOPE_ENABLED: AtomicBool = AtomicBool::new(true);

        static INTERNED_ROUTES: RwLock<Option<HashSet<&'static str>>> = RwLock::new(None);

        /// Disables or enables attributing SQL queries and HTTP requests to
        /// the axum route that triggered them.
        pub(crate) fn set_route_scope(enabled: bool) {
            ROUTE_SCOPE_ENABLED.store(enabled, Ordering::Relaxed);
        }

        pub(crate) fn route_scope_enabled() -> bool {
            ROUTE_SCOPE_ENABLED.load(Ordering::Relaxed)
        }

        /// Leaks each distinct route template once so the thread-local stays
        /// `Copy`; growth is bounded by the number of matched routes.
        pub(crate) fn intern_route(route: &str) -> &'static str {
            if let Some(found) = INTERNED_ROUTES
                .read()
                .unwrap()
                .as_ref()
                .and_then(|set| set.get(route).copied())
            {
                return found;
            }
            let _suspend = crate::lib_on::SuspendAllocTracking::new();
            let mut guard = INTERNED_ROUTES.write().unwrap();
            let set = guard.get_or_insert_with(HashSet::new);
            if let Some(found) = set.get(route) {
                return found;
            }
            let leaked: &'static str = Box::leak(route.to_owned().into_boxed_str());
            set.insert(leaked);
            leaked
        }

        /// Sets the current route for the duration of the returned guard and
        /// restores the previous value on drop, so nested layers and
        /// interleaved tasks on one runtime thread never observe a stale route.
        #[inline]
        pub(crate) fn enter_route(route: &'static str) -> RouteScopeGuard {
            let previous = CURRENT_ROUTE
                .try_with(|cell| cell.replace(Some(route)))
                .unwrap_or(None);
            RouteScopeGuard { previous }
        }

        #[inline]
        pub(crate) fn current_route() -> Option<&'static str> {
            CURRENT_ROUTE.try_with(|cell| cell.get()).ok().flatten()
        }

        pub(crate) struct RouteScopeGuard {
            previous: Option<&'static str>,
        }

        impl Drop for RouteScopeGuard {
            #[inline]
            fn drop(&mut self) {
                let _ = CURRENT_ROUTE.try_with(|cell| cell.set(self.previous));
            }
        }
    } else {
        #[inline]
        #[allow(dead_code)]
        pub(crate) fn current_route() -> Option<&'static str> {
            None
        }
    }
}

#[cfg(all(test, feature = "axum-0-8"))]
mod tests {
    use crate::lib_on::caller_stack::{current_route, enter_route, intern_route};

    #[test]
    fn nested_route_scopes_restore_previous() {
        assert_eq!(current_route(), None);
        let outer = intern_route("GET /outer");
        let inner = intern_route("GET /inner");
        assert!(std::ptr::eq(outer, intern_route("GET /outer")));
        {
            let _outer = enter_route(outer);
            assert_eq!(current_route(), Some(outer));
            {
                let _inner = enter_route(inner);
                assert_eq!(current_route(), Some(inner));
            }
            assert_eq!(current_route(), Some(outer));
        }
        assert_eq!(current_route(), None);
    }
}
