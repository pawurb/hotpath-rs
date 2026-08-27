//! Call-site location registry. Instrumentation macros register a static
//! [`Location`] under the same identity string used for stats aggregation
//! (function name, resource id string, debug loc); report builders join on
//! that string at build time, so the hot path never carries location data.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::json::JsonLocation;

/// Structured source location of an instrumented call site, captured at
/// compile time by the instrumentation macros.
pub struct Location {
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

static LOCATIONS: OnceLock<crate::lib_on::MetaRwLock<HashMap<&'static str, &'static Location>>> =
    OnceLock::new();

/// Registers `location` under `name`. First registration wins: two call sites
/// sharing one identity string (e.g. two `#[measure(label = "x")]` sites)
/// already merge into a single stats entry, and its location is whichever
/// site registered first.
#[doc(hidden)]
pub fn register_location(name: &'static str, location: &'static Location) {
    let map = LOCATIONS.get_or_init(|| crate::lib_on::meta_rw_lock!("locations", HashMap::new()));
    if let Ok(mut w) = map.write() {
        w.entry(name).or_insert(location);
    }
}

/// Joins an entry's identity string against the registry, normalizing the
/// file path for shipping in a report.
pub(crate) fn lookup_location(name: &str) -> Option<JsonLocation> {
    let map = LOCATIONS.get()?;
    let location = map.read().ok()?.get(name).copied()?;
    Some(JsonLocation {
        file: normalize_file_path(location.file),
        line: location.line,
        column: location.column,
    })
}

/// `file!()` is workspace-root-relative for workspace members and absolute
/// for registry dependencies. Relative paths pass through; absolute paths
/// under a cargo registry checkout are rewritten to
/// `<external>/<crate>-<version>/<path>` (keeps docs.rs linking possible and
/// strips `$HOME`); other absolute paths pass through - the server refuses to
/// link them.
fn normalize_file_path(file: &str) -> String {
    if !std::path::Path::new(file).is_absolute() {
        return file.to_string();
    }
    if let Some(pos) = file.find("/registry/src/") {
        let rest = &file[pos + "/registry/src/".len()..];
        // rest = "<index>/<crate>-<version>/<path>"
        if let Some((_index, crate_path)) = rest.split_once('/') {
            if !crate_path.is_empty() {
                return format!("<external>/{}", crate_path);
            }
        }
    }
    file.to_string()
}

#[cfg(test)]
mod tests {
    use crate::lib_on::locations::{
        lookup_location, normalize_file_path, register_location, Location,
    };

    #[test]
    fn relative_paths_pass_through() {
        assert_eq!(
            normalize_file_path("crates/hotpath/src/lib.rs"),
            "crates/hotpath/src/lib.rs"
        );
    }

    #[test]
    fn registry_paths_rewrite_to_external() {
        assert_eq!(
            normalize_file_path(
                "/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.47.1/src/lib.rs"
            ),
            "<external>/tokio-1.47.1/src/lib.rs"
        );
    }

    #[test]
    fn other_absolute_paths_pass_through() {
        assert_eq!(
            normalize_file_path("/home/u/project/src/main.rs"),
            "/home/u/project/src/main.rs"
        );
    }

    #[test]
    fn first_registration_wins() {
        static FIRST: Location = Location {
            file: "src/a.rs",
            line: 1,
            column: 2,
        };
        static SECOND: Location = Location {
            file: "src/b.rs",
            line: 3,
            column: 4,
        };
        register_location("locations_test_first_wins", &FIRST);
        register_location("locations_test_first_wins", &SECOND);

        let found = lookup_location("locations_test_first_wins").unwrap();
        assert_eq!(found.file, "src/a.rs");
        assert_eq!(found.line, 1);
        assert_eq!(found.column, 2);

        assert!(lookup_location("locations_test_unknown").is_none());
    }
}
