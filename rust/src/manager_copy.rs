//! Annotation-manager copy transition.

use crate::format::format_annotations;
use crate::types::Annotation;

/// Whether the annotation manager should close or remain visible after a copy attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerCopyOutcome {
    Close,
    StayOpen { message: String },
}

/// Format and copy annotations without changing the supplied annotations or their store.
pub fn copy_annotations(
    annotations: &[Annotation],
    write_clipboard: impl FnOnce(&str) -> Result<(), String>,
) -> ManagerCopyOutcome {
    if annotations.is_empty() {
        return ManagerCopyOutcome::StayOpen {
            message: "Nothing to copy.".to_owned(),
        };
    }
    match write_clipboard(&format_annotations(annotations)) {
        Ok(()) => ManagerCopyOutcome::Close,
        Err(message) => ManagerCopyOutcome::StayOpen { message },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use crate::types::{Annotation, InvocationContext};

    use super::*;

    fn annotation(id: &str) -> Annotation {
        Annotation {
            selected_text: format!("selection {id}"),
            context: InvocationContext::default(),
            captured_at: "captured".to_owned(),
            id: id.to_owned(),
            comment: format!("comment {id}"),
            created_at: "created".to_owned(),
        }
    }

    #[test]
    fn successful_copy_closes_without_changing_annotations() {
        let annotations = vec![annotation("one"), annotation("two")];
        let original = annotations.clone();
        let clipboard = RefCell::new(String::new());
        let outcome = copy_annotations(&annotations, |text| {
            text.clone_into(&mut clipboard.borrow_mut());
            Ok(())
        });
        assert_eq!(outcome, ManagerCopyOutcome::Close);
        assert!(clipboard.borrow().contains("selection one"));
        assert!(clipboard.borrow().contains("selection two"));
        assert_eq!(annotations, original);
    }

    #[test]
    fn clipboard_failure_stays_open() {
        let outcome = copy_annotations(&[annotation("one")], |_| {
            Err("Clipboard unavailable".to_owned())
        });
        assert_eq!(
            outcome,
            ManagerCopyOutcome::StayOpen {
                message: "Clipboard unavailable".to_owned()
            }
        );
    }

    #[test]
    fn empty_copy_stays_open_without_writing() {
        let writes = Cell::new(0);
        let outcome = copy_annotations(&[], |_| {
            writes.set(writes.get() + 1);
            Ok(())
        });
        assert_eq!(
            outcome,
            ManagerCopyOutcome::StayOpen {
                message: "Nothing to copy.".to_owned()
            }
        );
        assert_eq!(writes.get(), 0);
    }
}
