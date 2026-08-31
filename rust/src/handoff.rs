//! One-shot selection handoff for remote/headless Herdr sessions.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::types::javascript_trim;

/// A handed-off selection older than this is ignored.
pub const HANDOFF_MAX_AGE: Duration = Duration::from_secs(15);

/// `$XDG_RUNTIME_DIR` when set, else the system temp dir, plus the uid.
pub fn handoff_path() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map_or_else(std::env::temp_dir, PathBuf::from);
    base.join(format!("herdr-annotate-{}", current_user_id()))
        .join("selection")
}

#[cfg(unix)]
fn current_user_id() -> String {
    rustix::process::getuid().as_raw().to_string()
}

#[cfg(not(unix))]
fn current_user_id() -> String {
    "user".to_owned()
}

/// Return fresh, non-blank handed-off text and remove the file whether fresh or stale.
pub fn take_handoff(file: &Path, now: SystemTime, max_age: Duration) -> Option<String> {
    let metadata = std::fs::metadata(file).ok()?;
    let fresh = metadata.is_file()
        && metadata
            .modified()
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() <= max_age);
    let text = fresh
        .then(|| {
            std::fs::read(file)
                .ok()
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        })
        .flatten();
    let _ = std::fs::remove_file(file);
    text.filter(|value| !javascript_trim(value).is_empty())
}

/// Take a selection from the default handoff file.
pub fn take_default_handoff() -> Option<String> {
    take_handoff(&handoff_path(), SystemTime::now(), HANDOFF_MAX_AGE)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert by panicking")]

    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

    fn temporary_file() -> PathBuf {
        let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "herdr-annotate-handoff-{}-{sequence}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporary directory");
        dir.join("selection")
    }

    #[test]
    fn handoff_path_is_per_user_and_prefers_runtime_directory_when_present() {
        let path = handoff_path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("selection")
        );
        assert!(
            path.parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("herdr-annotate-"))
        );
    }

    #[test]
    fn fresh_text_is_returned_and_removed() {
        let file = temporary_file();
        std::fs::write(&file, "hello\nworld\n").expect("fixture");
        assert_eq!(
            take_handoff(&file, SystemTime::now(), HANDOFF_MAX_AGE).as_deref(),
            Some("hello\nworld\n")
        );
        assert!(!file.exists());
    }

    #[test]
    fn future_timestamps_are_fresh_like_the_typescript_age_check() {
        let file = temporary_file();
        std::fs::write(&file, "new").expect("fixture");
        assert_eq!(
            take_handoff(&file, SystemTime::UNIX_EPOCH, HANDOFF_MAX_AGE).as_deref(),
            Some("new")
        );
        assert!(!file.exists());
    }

    #[test]
    fn invalid_utf8_is_replaced_like_node_utf8_decoding() {
        let file = temporary_file();
        std::fs::write(&file, b"invalid-\xff-handoff").expect("fixture");
        assert_eq!(
            take_handoff(&file, SystemTime::now(), HANDOFF_MAX_AGE).as_deref(),
            Some("invalid-�-handoff")
        );
        assert!(!file.exists());
    }

    #[test]
    fn stale_blank_and_missing_files_are_ignored_and_removed() {
        let stale = temporary_file();
        std::fs::write(&stale, "old").expect("fixture");
        assert_eq!(
            take_handoff(
                &stale,
                SystemTime::now() + HANDOFF_MAX_AGE + Duration::from_secs(1),
                HANDOFF_MAX_AGE
            ),
            None
        );
        assert!(!stale.exists());
        let blank = temporary_file();
        std::fs::write(&blank, "  \n").expect("fixture");
        assert_eq!(
            take_handoff(&blank, SystemTime::now(), HANDOFF_MAX_AGE),
            None
        );
        assert_eq!(
            take_handoff(&blank, SystemTime::now(), HANDOFF_MAX_AGE),
            None
        );
    }
}
