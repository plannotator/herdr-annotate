//! Concurrent, byte-compatible JSONL annotation stores.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::paths::{annotations_path, archives_path};
use crate::types::{
    Annotation, ArchivedAnnotationSet, InvocationContext, parse_annotation,
    parse_archived_annotation_set,
};

const STALE_LOCK: Duration = Duration::from_secs(30);

/// The result of an expected annotation-store operation.
pub type StoreResult<T> = Result<T, String>;

#[derive(Debug, Clone, Copy)]
enum StoreName {
    Annotations,
    Archives,
}

impl StoreName {
    const fn lower(self) -> &'static str {
        match self {
            Self::Annotations => "annotations",
            Self::Archives => "archives",
        }
    }

    const fn capitalized(self) -> &'static str {
        match self {
            Self::Annotations => "Annotations",
            Self::Archives => "Archives",
        }
    }
}

#[derive(Debug)]
struct StoreLockLease {
    path: PathBuf,
    owner: String,
}

impl Drop for StoreLockLease {
    fn drop(&mut self) {
        let owner_path = self.path.join("owner");
        let Ok(current_owner) = fs::read_to_string(owner_path) else {
            return;
        };
        if current_owner.trim() == self.owner {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Present append-ordered annotations with the most recently saved first.
pub fn newest_first_annotations(annotations: &[Annotation]) -> Vec<Annotation> {
    annotations.iter().rev().cloned().collect()
}

/// Load the complete active store, rejecting malformed records instead of dropping data.
pub fn load_annotations(dir: &Path) -> StoreResult<Vec<Annotation>> {
    with_store_lock(dir, StoreName::Annotations, || {
        load_annotations_unlocked(dir)
    })
}

/// Append one annotation without rewriting existing records.
pub fn append_annotation(dir: &Path, annotation: &Annotation) -> StoreResult<()> {
    append_annotation_record(dir, annotation)
}

/// Append using the property insertion order of TypeScript's invocation-context editor fallback.
pub(crate) fn append_annotation_context_first(
    dir: &Path,
    annotation: &Annotation,
) -> StoreResult<()> {
    let context_first = ContextFirstAnnotation {
        selected_text: &annotation.selected_text,
        context: &annotation.context,
        captured_at: &annotation.captured_at,
        id: &annotation.id,
        comment: &annotation.comment,
        created_at: &annotation.created_at,
    };
    append_annotation_record(dir, &context_first)
}

fn append_annotation_record(dir: &Path, annotation: &impl Serialize) -> StoreResult<()> {
    with_store_lock(dir, StoreName::Annotations, || {
        let mut file = append_file(&annotations_path(dir))
            .map_err(|error| safe_file_error("Unable to save annotation", &error))?;
        let record = serde_json::to_string(annotation)
            .map_err(|_| "Unable to save annotation".to_owned())?;
        writeln!(file, "{record}")
            .map_err(|error| safe_file_error("Unable to save annotation", &error))
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextFirstAnnotation<'a> {
    selected_text: &'a str,
    context: &'a InvocationContext,
    captured_at: &'a str,
    id: &'a str,
    comment: &'a str,
    created_at: &'a str,
}

/// Remove selected annotation IDs without racing concurrent annotation saves.
pub fn remove_annotations_by_id(dir: &Path, annotation_ids: &[String]) -> StoreResult<()> {
    with_store_lock(dir, StoreName::Annotations, || {
        let loaded = load_annotations_unlocked(dir)?;
        let removed = annotation_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let retained = loaded
            .into_iter()
            .filter(|annotation| !removed.contains(annotation.id.as_str()))
            .collect::<Vec<_>>();
        replace_annotations_unlocked(dir, &retained)
    })
}

/// Merge annotations into the active list without duplicating existing annotation IDs.
pub fn merge_annotations(dir: &Path, annotations: &[Annotation]) -> StoreResult<usize> {
    with_store_lock(dir, StoreName::Annotations, || {
        let mut loaded = load_annotations_unlocked(dir)?;
        let existing_ids = loaded
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        let additions = annotations
            .iter()
            .filter(|item| !existing_ids.contains(item.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if additions.is_empty() {
            return Ok(0);
        }
        let count = additions.len();
        loaded.extend(additions);
        replace_annotations_unlocked(dir, &loaded)?;
        Ok(count)
    })
}

/// Present append-ordered archive sets with the most recently archived first.
pub fn newest_first_archived_sets(
    archives: &[ArchivedAnnotationSet],
) -> Vec<ArchivedAnnotationSet> {
    archives.iter().rev().cloned().collect()
}

/// Load complete, parsed annotation sets from the archive store.
pub fn load_archived_sets(dir: &Path) -> StoreResult<Vec<ArchivedAnnotationSet>> {
    with_store_lock(dir, StoreName::Archives, || {
        load_archived_sets_unlocked(dir)
    })
}

/// Atomically append one complete set to the archive store.
pub fn append_archived_set(dir: &Path, archive: &ArchivedAnnotationSet) -> StoreResult<()> {
    with_store_lock(dir, StoreName::Archives, || {
        let mut loaded = load_archived_sets_unlocked(dir)?;
        loaded.push(archive.clone());
        replace_archived_sets_unlocked(dir, &loaded)
    })
}

/// Permanently remove one archived set by its archive ID.
pub fn remove_archived_set(dir: &Path, archive_id: &str) -> StoreResult<()> {
    with_store_lock(dir, StoreName::Archives, || {
        let retained = load_archived_sets_unlocked(dir)?
            .into_iter()
            .filter(|archive| archive.id != archive_id)
            .collect::<Vec<_>>();
        replace_archived_sets_unlocked(dir, &retained)
    })
}

fn load_annotations_unlocked(dir: &Path) -> StoreResult<Vec<Annotation>> {
    load_json_lines(&annotations_path(dir), "annotations", parse_annotation)
}

fn replace_annotations_unlocked(dir: &Path, annotations: &[Annotation]) -> StoreResult<()> {
    replace_json_lines(
        dir,
        &annotations_path(dir),
        "annotations",
        annotations,
        "Unable to update annotations",
    )
}

fn load_archived_sets_unlocked(dir: &Path) -> StoreResult<Vec<ArchivedAnnotationSet>> {
    load_json_lines(
        &archives_path(dir),
        "archives",
        parse_archived_annotation_set,
    )
}

fn replace_archived_sets_unlocked(
    dir: &Path,
    archives: &[ArchivedAnnotationSet],
) -> StoreResult<()> {
    replace_json_lines(
        dir,
        &archives_path(dir),
        "archives",
        archives,
        "Unable to update archives",
    )
}

fn load_json_lines<T>(
    file: &Path,
    label: &str,
    parse: fn(&Value) -> Option<T>,
) -> StoreResult<Vec<T>> {
    let opened = match File::open(file) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(safe_file_error(&format!("Unable to read {label}"), &error)),
    };
    let mut records = Vec::new();
    for line in BufReader::new(opened).lines() {
        let line =
            line.map_err(|error| safe_file_error(&format!("Unable to read {label}"), &error))?;
        if line.is_empty() {
            continue;
        }
        let decoded = serde_json::from_str::<Value>(&line)
            .map_err(|_| format!("Unable to read {label} (invalid data)"))?;
        let record =
            parse(&decoded).ok_or_else(|| format!("Unable to read {label} (invalid data)"))?;
        records.push(record);
    }
    Ok(records)
}

fn replace_json_lines<T: Serialize>(
    dir: &Path,
    file: &Path,
    temporary_label: &str,
    records: &[T],
    error_message: &str,
) -> StoreResult<()> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temporary = dir.join(format!(
        ".{temporary_label}-{}-{millis}.tmp",
        std::process::id()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut output = private_file(&temporary, false)?;
        for record in records {
            serde_json::to_writer(&mut output, record).map_err(std::io::Error::other)?;
            output.write_all(b"\n")?;
        }
        output.flush()?;
        drop(output);
        fs::rename(&temporary, file)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(safe_file_error(error_message, &error));
    }
    Ok(())
}

fn with_store_lock<T>(
    dir: &Path,
    store: StoreName,
    operation: impl FnOnce() -> StoreResult<T>,
) -> StoreResult<T> {
    create_private_dir_all(dir)
        .map_err(|error| safe_file_error(&format!("Unable to access {}", store.lower()), &error))?;
    let _lease = acquire_store_lock(&dir.join(format!(".{}.lock", store.lower())), store)?;
    operation()
}

fn acquire_store_lock(lock: &Path, store: StoreName) -> StoreResult<StoreLockLease> {
    let owner = format!("{}:{}", std::process::id(), Uuid::new_v4());
    match create_store_lock(lock, &owner) {
        Ok(()) => {
            return Ok(StoreLockLease {
                path: lock.to_path_buf(),
                owner,
            });
        }
        Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => {
            return Err(safe_file_error(
                &format!("Unable to lock {}", store.lower()),
                &error,
            ));
        }
        Err(_) => {}
    }
    if !is_stale_lock(lock) {
        return Err(format!("{} are busy; try again.", store.capitalized()));
    }
    fs::remove_dir_all(lock)
        .map_err(|error| safe_file_error(&format!("Unable to lock {}", store.lower()), &error))?;
    match create_store_lock(lock, &owner) {
        Ok(()) => Ok(StoreLockLease {
            path: lock.to_path_buf(),
            owner,
        }),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(format!("{} are busy; try again.", store.capitalized()))
        }
        Err(error) => Err(safe_file_error(
            &format!("Unable to lock {}", store.lower()),
            &error,
        )),
    }
}

fn create_store_lock(lock: &Path, owner: &str) -> std::io::Result<()> {
    create_private_dir(lock)?;
    let owner_path = lock.join("owner");
    let result = private_file(&owner_path, false)
        .and_then(|mut file| file.write_all(format!("{owner}\n").as_bytes()));
    if let Err(error) = result {
        let _ = fs::remove_dir_all(lock);
        return Err(error);
    }
    Ok(())
}

fn is_stale_lock(lock: &Path) -> bool {
    fs::metadata(lock)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_LOCK)
}

fn append_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    set_private_open_options(&mut options);
    options.open(path)
}

fn private_file(path: &Path, append: bool) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .create(true)
        .write(true)
        .truncate(!append)
        .append(append);
    set_private_open_options(&mut options);
    options.open(path)
}

#[cfg(unix)]
fn set_private_open_options(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_open_options(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)
}

fn safe_file_error(prefix: &str, error: &std::io::Error) -> String {
    error.raw_os_error().map_or_else(
        || prefix.to_owned(),
        |code| format!("{prefix} ({})", os_error_code(error.kind(), code)),
    )
}

fn os_error_code(kind: std::io::ErrorKind, raw: i32) -> String {
    match kind {
        std::io::ErrorKind::NotFound => "ENOENT".to_owned(),
        std::io::ErrorKind::PermissionDenied => "EACCES".to_owned(),
        std::io::ErrorKind::AlreadyExists => "EEXIST".to_owned(),
        std::io::ErrorKind::IsADirectory => "EISDIR".to_owned(),
        std::io::ErrorKind::NotADirectory => "ENOTDIR".to_owned(),
        std::io::ErrorKind::DirectoryNotEmpty => "ENOTEMPTY".to_owned(),
        std::io::ErrorKind::StorageFull => "ENOSPC".to_owned(),
        std::io::ErrorKind::ReadOnlyFilesystem => "EROFS".to_owned(),
        std::io::ErrorKind::InvalidInput => "EINVAL".to_owned(),
        _ => format!("OS error {raw}"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert by panicking")]

    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::types::InvocationContext;

    use super::*;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn temporary_directory() -> PathBuf {
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "herdr-annotate-store-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temporary directory");
        dir
    }

    fn annotation(id: &str) -> Annotation {
        Annotation {
            selected_text: format!("selection {id}"),
            context: InvocationContext::default(),
            captured_at: "2026-08-08T00:00:00Z".to_owned(),
            id: id.to_owned(),
            comment: format!("comment {id}"),
            created_at: "2026-08-08T00:00:01Z".to_owned(),
        }
    }

    fn archive(id: &str, annotation_ids: &[&str]) -> ArchivedAnnotationSet {
        ArchivedAnnotationSet {
            version: 1,
            id: id.to_owned(),
            archived_at: format!("2026-08-26T23:32:0{}Z", id.len()),
            annotations: annotation_ids.iter().map(|item| annotation(item)).collect(),
        }
    }

    #[test]
    fn newest_first_does_not_mutate_storage_order() {
        let stored = vec![annotation("one"), annotation("two"), annotation("three")];
        let newest = newest_first_annotations(&stored);
        assert_eq!(
            newest
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["three", "two", "one"]
        );
        assert_eq!(
            stored
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["one", "two", "three"]
        );
    }

    #[test]
    fn annotations_append_load_remove_and_merge() {
        let dir = temporary_directory();
        append_annotation(&dir, &annotation("one")).expect("append one");
        append_annotation(&dir, &annotation("two")).expect("append two");
        assert_eq!(
            load_annotations(&dir).expect("load"),
            [annotation("one"), annotation("two")]
        );
        remove_annotations_by_id(&dir, &["one".to_owned()]).expect("remove");
        assert_eq!(load_annotations(&dir).expect("load"), [annotation("two")]);
        assert_eq!(
            merge_annotations(&dir, &[annotation("two"), annotation("three")]).expect("merge"),
            1
        );
        assert_eq!(
            load_annotations(&dir).expect("load"),
            [annotation("two"), annotation("three")]
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invocation_fallback_append_preserves_typescript_property_order() {
        let dir = temporary_directory();
        append_annotation_context_first(&dir, &annotation("one")).expect("append");
        assert_eq!(
            fs::read_to_string(annotations_path(&dir)).expect("record"),
            concat!(
                "{\"selectedText\":\"selection one\",\"context\":{},",
                "\"capturedAt\":\"2026-08-08T00:00:00Z\",\"id\":\"one\",",
                "\"comment\":\"comment one\",\"createdAt\":\"2026-08-08T00:00:01Z\"}\n"
            )
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn store_creation_uses_the_process_default_directory_mode() {
        use std::os::unix::fs::PermissionsExt;

        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let parent = std::env::temp_dir().join(format!(
            "herdr-annotate-store-parent-{}-{sequence}",
            std::process::id()
        ));
        let dir = parent.join("state");
        let reference = parent.join("reference");
        let _ = fs::remove_dir_all(&parent);
        fs::create_dir_all(&parent).expect("temporary parent");
        fs::create_dir(&reference).expect("reference directory");

        assert!(load_annotations(&dir).expect("load").is_empty());
        assert_eq!(
            fs::metadata(&dir)
                .expect("state metadata")
                .permissions()
                .mode()
                & 0o777,
            fs::metadata(reference)
                .expect("reference metadata")
                .permissions()
                .mode()
                & 0o777
        );
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn malformed_active_data_is_rejected() {
        let dir = temporary_directory();
        fs::write(annotations_path(&dir), "{broken\n").expect("fixture");
        assert_eq!(
            load_annotations(&dir),
            Err("Unable to read annotations (invalid data)".to_owned())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn contention_fails_safely_and_abandoned_locks_recover() {
        let dir = temporary_directory();
        let lock = dir.join(".annotations.lock");
        fs::create_dir(&lock).expect("lock");
        assert_eq!(
            append_annotation(&dir, &annotation("one")),
            Err("Annotations are busy; try again.".to_owned())
        );
        let stale = SystemTime::now() - Duration::from_secs(31);
        let file = File::open(&lock).expect("lock handle");
        file.set_times(fs::FileTimes::new().set_modified(stale))
            .expect("age lock");
        append_annotation(&dir, &annotation("one")).expect("recover");
        assert_eq!(load_annotations(&dir).expect("load"), [annotation("one")]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn archive_sets_persist_newest_first_and_remove_individually() {
        let dir = temporary_directory();
        let first = archive("one", &["annotation-one"]);
        let second = archive("two", &["annotation-two", "annotation-three"]);
        append_archived_set(&dir, &first).expect("archive one");
        append_archived_set(&dir, &second).expect("archive two");
        let loaded = load_archived_sets(&dir).expect("load archives");
        assert_eq!(loaded, [first, second.clone()]);
        assert_eq!(newest_first_archived_sets(&loaded).first(), Some(&second));
        remove_archived_set(&dir, "one").expect("remove archive");
        assert_eq!(load_archived_sets(&dir).expect("load"), [second]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_archive_data_is_rejected() {
        let dir = temporary_directory();
        fs::write(archives_path(&dir), "{broken\n").expect("fixture");
        assert_eq!(
            load_archived_sets(&dir),
            Err("Unable to read archives (invalid data)".to_owned())
        );
        let _ = fs::remove_dir_all(dir);
    }
}
