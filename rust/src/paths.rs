//! Plugin and persisted-store paths.

use std::path::{Path, PathBuf};

/// Remove Windows extended-path prefixes that process launchers cannot use as a cwd.
pub fn normalize_windows_path(value: &str) -> String {
    if let Some(without_prefix) = value.strip_prefix(r"\\?\") {
        return without_prefix
            .get(..4)
            .filter(|prefix| prefix.eq_ignore_ascii_case("UNC\\"))
            .map_or_else(
                || without_prefix.to_owned(),
                |_| format!(r"\\{}", without_prefix.get(4..).unwrap_or_default()),
            );
    }
    if let Some(without_prefix) = value.strip_prefix("//?/") {
        return without_prefix
            .get(..4)
            .filter(|prefix| prefix.eq_ignore_ascii_case("UNC/"))
            .map_or_else(
                || without_prefix.to_owned(),
                |_| format!("//{}", without_prefix.get(4..).unwrap_or_default()),
            );
    }
    value.to_owned()
}

/// Return Herdr's plugin-owned state directory when the runtime supplied one.
pub fn state_dir() -> Option<PathBuf> {
    std::env::var_os("HERDR_PLUGIN_STATE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Return Herdr's plugin root in a process-safe form.
pub fn plugin_root() -> Option<PathBuf> {
    std::env::var("HERDR_PLUGIN_ROOT")
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| PathBuf::from(normalize_windows_path(&value)))
}

/// Resolve the JSONL store inside a plugin state directory.
pub fn annotations_path(dir: &Path) -> PathBuf {
    dir.join("annotations.jsonl")
}

/// Resolve the archived-set JSONL store inside a plugin state directory.
pub fn archives_path(dir: &Path) -> PathBuf {
    dir.join("archives.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_windows_paths_are_normalized() {
        assert_eq!(normalize_windows_path(r"\\?\C:\foo"), r"C:\foo");
        assert_eq!(
            normalize_windows_path(r"\\?\UNC\server\share\foo"),
            r"\\server\share\foo"
        );
        assert_eq!(normalize_windows_path("//?/C:/foo"), "C:/foo");
        assert_eq!(
            normalize_windows_path("//?/UNC/server/share/foo"),
            "//server/share/foo"
        );
    }

    #[test]
    fn ordinary_and_empty_paths_are_unchanged() {
        assert_eq!(normalize_windows_path(r"C:\foo"), r"C:\foo");
        assert_eq!(
            normalize_windows_path("/home/user/plugin"),
            "/home/user/plugin"
        );
        assert_eq!(normalize_windows_path(""), "");
    }
}
