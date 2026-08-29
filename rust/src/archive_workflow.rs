//! Recoverable copy/archive and restore transitions.

use crate::format::format_annotations;
use crate::store::{StoreResult, newest_first_annotations};
use crate::types::{Annotation, ArchivedAnnotationSet};

/// Dependencies for one copy-and-archive transition.
#[derive(Debug)]
pub struct CopyAndArchiveDependencies<Load, Write, Save, Remove, Id, Now> {
    pub load_active: Load,
    pub write_clipboard: Write,
    pub save_archive: Save,
    pub remove_active: Remove,
    pub create_archive_id: Id,
    pub now: Now,
}

/// The manager transition produced by a copy-and-archive attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyAndArchiveOutcome {
    Close { archived_count: usize },
    StayOpen { message: String },
    ArchivedActiveRetained { message: String },
}

/// Copy the active annotations, persist a recoverable archive, then remove active IDs.
pub fn copy_and_archive_annotations<Load, Write, Save, Remove, Id, Now>(
    dependencies: CopyAndArchiveDependencies<Load, Write, Save, Remove, Id, Now>,
) -> CopyAndArchiveOutcome
where
    Load: FnOnce() -> StoreResult<Vec<Annotation>>,
    Write: FnOnce(String) -> Result<(), String>,
    Save: FnOnce(ArchivedAnnotationSet) -> StoreResult<()>,
    Remove: FnOnce(Vec<String>) -> StoreResult<()>,
    Id: FnOnce() -> String,
    Now: FnOnce() -> String,
{
    let active = match (dependencies.load_active)() {
        Ok(active) => active,
        Err(message) => return CopyAndArchiveOutcome::StayOpen { message },
    };
    if active.is_empty() {
        return CopyAndArchiveOutcome::StayOpen {
            message: "Nothing to copy and archive.".to_owned(),
        };
    }
    if let Err(message) =
        (dependencies.write_clipboard)(format_annotations(&newest_first_annotations(&active)))
    {
        return CopyAndArchiveOutcome::StayOpen { message };
    }
    let archive = ArchivedAnnotationSet {
        version: 1,
        id: (dependencies.create_archive_id)(),
        archived_at: (dependencies.now)(),
        annotations: active.clone(),
    };
    if let Err(message) = (dependencies.save_archive)(archive) {
        return CopyAndArchiveOutcome::StayOpen { message };
    }
    let ids = active
        .iter()
        .map(|annotation| annotation.id.clone())
        .collect::<Vec<_>>();
    if let Err(message) = (dependencies.remove_active)(ids) {
        return CopyAndArchiveOutcome::ArchivedActiveRetained { message };
    }
    CopyAndArchiveOutcome::Close {
        archived_count: active.len(),
    }
}

/// Dependencies for one restore transition.
#[derive(Debug)]
pub struct RestoreArchiveDependencies<Merge, Remove> {
    pub merge_active: Merge,
    pub remove_archive: Remove,
}

/// The manager transition produced by restoring one archived annotation set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreArchivedSetOutcome {
    Restored {
        restored_count: usize,
    },
    StayOpen {
        message: String,
    },
    RestoredArchiveRetained {
        restored_count: usize,
        message: String,
    },
}

/// Merge an archived set into active annotations, then remove its archive record.
pub fn restore_archived_set<Merge, Remove>(
    archive: &ArchivedAnnotationSet,
    dependencies: RestoreArchiveDependencies<Merge, Remove>,
) -> RestoreArchivedSetOutcome
where
    Merge: FnOnce(Vec<Annotation>) -> StoreResult<usize>,
    Remove: FnOnce(String) -> StoreResult<()>,
{
    let restored_count = match (dependencies.merge_active)(archive.annotations.clone()) {
        Ok(count) => count,
        Err(message) => return RestoreArchivedSetOutcome::StayOpen { message },
    };
    match (dependencies.remove_archive)(archive.id.clone()) {
        Ok(()) => RestoreArchivedSetOutcome::Restored { restored_count },
        Err(message) => RestoreArchivedSetOutcome::RestoredArchiveRetained {
            restored_count,
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert by panicking")]

    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::store::{
        append_annotation, append_archived_set, load_annotations, load_archived_sets,
        merge_annotations, remove_annotations_by_id, remove_archived_set,
    };
    use crate::types::InvocationContext;

    use super::*;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn temporary_directory() -> PathBuf {
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "herdr-annotate-workflow-{}-{sequence}",
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

    fn archive(ids: &[&str]) -> ArchivedAnnotationSet {
        ArchivedAnnotationSet {
            version: 1,
            id: "archive-one".to_owned(),
            archived_at: "2026-08-26T23:32:00Z".to_owned(),
            annotations: ids.iter().map(|id| annotation(id)).collect(),
        }
    }

    #[test]
    fn copy_archives_in_order_before_removing_active_ids() {
        let events = RefCell::new(Vec::new());
        let clipboard = RefCell::new(String::new());
        let saved = RefCell::new(None);
        let removed = RefCell::new(Vec::new());
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || {
                events.borrow_mut().push("load");
                Ok(vec![annotation("one"), annotation("two")])
            },
            write_clipboard: |text: String| {
                events.borrow_mut().push("copy");
                text.clone_into(&mut clipboard.borrow_mut());
                Ok(())
            },
            save_archive: |archive: ArchivedAnnotationSet| {
                events.borrow_mut().push("archive");
                saved.replace(Some(archive));
                Ok(())
            },
            remove_active: |ids: Vec<String>| {
                events.borrow_mut().push("remove");
                ids.clone_into(&mut removed.borrow_mut());
                Ok(())
            },
            create_archive_id: || "archive-one".to_owned(),
            now: || "2026-08-26T23:32:00Z".to_owned(),
        });
        assert_eq!(outcome, CopyAndArchiveOutcome::Close { archived_count: 2 });
        assert_eq!(*events.borrow(), ["load", "copy", "archive", "remove"]);
        assert!(
            clipboard.borrow().find("selection two") < clipboard.borrow().find("selection one")
        );
        assert_eq!(saved.borrow().as_ref(), Some(&archive(&["one", "two"])));
        assert_eq!(*removed.borrow(), ["one", "two"]);
    }

    #[test]
    fn copy_failure_does_not_archive_or_clear() {
        let archived = Cell::new(false);
        let removed = Cell::new(false);
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || Ok(vec![annotation("one")]),
            write_clipboard: |_| Err("Clipboard unavailable".to_owned()),
            save_archive: |_| {
                archived.set(true);
                Ok(())
            },
            remove_active: |_| {
                removed.set(true);
                Ok(())
            },
            create_archive_id: || "archive-one".to_owned(),
            now: || "now".to_owned(),
        });
        assert_eq!(
            outcome,
            CopyAndArchiveOutcome::StayOpen {
                message: "Clipboard unavailable".to_owned()
            }
        );
        assert!(!archived.get() && !removed.get());
    }

    #[test]
    fn archive_failure_does_not_clear_active() {
        let removed = Cell::new(false);
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || Ok(vec![annotation("one")]),
            write_clipboard: |_| Ok(()),
            save_archive: |_| Err("Archive unavailable".to_owned()),
            remove_active: |_| {
                removed.set(true);
                Ok(())
            },
            create_archive_id: || "archive-one".to_owned(),
            now: || "now".to_owned(),
        });
        assert_eq!(
            outcome,
            CopyAndArchiveOutcome::StayOpen {
                message: "Archive unavailable".to_owned()
            }
        );
        assert!(!removed.get());
    }

    #[test]
    fn clear_failure_reports_retained_active_data() {
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || Ok(vec![annotation("one")]),
            write_clipboard: |_| Ok(()),
            save_archive: |_| Ok(()),
            remove_active: |_| Err("Active store unavailable".to_owned()),
            create_archive_id: || "archive-one".to_owned(),
            now: || "now".to_owned(),
        });
        assert_eq!(
            outcome,
            CopyAndArchiveOutcome::ArchivedActiveRetained {
                message: "Active store unavailable".to_owned()
            }
        );
    }

    #[test]
    fn restore_merges_before_removing_archive_and_reports_partial_failure() {
        let events = RefCell::new(Vec::new());
        let restored = restore_archived_set(
            &archive(&["one", "two"]),
            RestoreArchiveDependencies {
                merge_active: |_| {
                    events.borrow_mut().push("merge");
                    Ok(2)
                },
                remove_archive: |_| {
                    events.borrow_mut().push("remove");
                    Ok(())
                },
            },
        );
        assert_eq!(
            restored,
            RestoreArchivedSetOutcome::Restored { restored_count: 2 }
        );
        assert_eq!(*events.borrow(), ["merge", "remove"]);

        let retained = restore_archived_set(
            &archive(&["one"]),
            RestoreArchiveDependencies {
                merge_active: |_| Ok(1),
                remove_archive: |_| Err("Archive unavailable".to_owned()),
            },
        );
        assert_eq!(
            retained,
            RestoreArchivedSetOutcome::RestoredArchiveRetained {
                restored_count: 1,
                message: "Archive unavailable".to_owned()
            }
        );
    }

    #[test]
    fn restore_failure_keeps_archive() {
        let removed = Cell::new(false);
        let outcome = restore_archived_set(
            &archive(&["one"]),
            RestoreArchiveDependencies {
                merge_active: |_| Err("Active store unavailable".to_owned()),
                remove_archive: |_| {
                    removed.set(true);
                    Ok(())
                },
            },
        );
        assert_eq!(
            outcome,
            RestoreArchivedSetOutcome::StayOpen {
                message: "Active store unavailable".to_owned()
            }
        );
        assert!(!removed.get());
    }

    #[test]
    fn real_stores_archive_clear_restore_and_preserve_concurrent_saves() {
        let dir = temporary_directory();
        append_annotation(&dir, &annotation("snapshot")).expect("append");
        let outcome = copy_and_archive_annotations(CopyAndArchiveDependencies {
            load_active: || load_annotations(&dir),
            write_clipboard: |_| Ok(()),
            save_archive: |set| {
                append_archived_set(&dir, &set)?;
                append_annotation(&dir, &annotation("concurrent"))
            },
            remove_active: |ids: Vec<String>| remove_annotations_by_id(&dir, &ids),
            create_archive_id: || "archive-one".to_owned(),
            now: || "2026-08-26T23:32:00Z".to_owned(),
        });
        assert_eq!(outcome, CopyAndArchiveOutcome::Close { archived_count: 1 });
        assert_eq!(
            load_annotations(&dir).expect("active"),
            [annotation("concurrent")]
        );
        let stored = load_archived_sets(&dir).expect("archives");
        assert_eq!(
            stored.first().map(|set| set.annotations.as_slice()),
            Some([annotation("snapshot")].as_slice())
        );
        let target = stored.first().expect("archive");
        let restored = restore_archived_set(
            target,
            RestoreArchiveDependencies {
                merge_active: |items: Vec<Annotation>| merge_annotations(&dir, &items),
                remove_archive: |id: String| remove_archived_set(&dir, &id),
            },
        );
        assert_eq!(
            restored,
            RestoreArchivedSetOutcome::Restored { restored_count: 1 }
        );
        assert_eq!(load_archived_sets(&dir).expect("archives"), []);
        let _ = fs::remove_dir_all(dir);
    }
}
