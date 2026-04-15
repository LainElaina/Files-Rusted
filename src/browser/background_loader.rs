use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use crate::AppWindow;

use super::{pathing::load_directory_entries, DirectoryEntry};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct LoadGeneration(pub(super) u64);

#[derive(Clone, Debug)]
pub(super) struct DirectoryLoadResult<T> {
    pub(super) generation: LoadGeneration,
    pub(super) target_path: PathBuf,
    pub(super) outcome: Result<T, DirectoryLoadError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DirectoryLoadError {
    pub(super) message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoadResultAction {
    ApplySuccess,
    ApplyFailure,
    DropStale,
}

pub(super) type SharedLoadResults<T> = Arc<Mutex<Vec<DirectoryLoadResult<T>>>>;

pub(super) fn classify_result_for_application<T>(
    current_generation: LoadGeneration,
    result: &DirectoryLoadResult<T>,
) -> LoadResultAction {
    if result.generation != current_generation {
        return LoadResultAction::DropStale;
    }

    match result.outcome {
        Ok(_) => LoadResultAction::ApplySuccess,
        Err(_) => LoadResultAction::ApplyFailure,
    }
}

pub(super) fn spawn_directory_load(
    window_weak: slint::Weak<AppWindow>,
    results: SharedLoadResults<Vec<DirectoryEntry>>,
    target_path: PathBuf,
    generation: LoadGeneration,
) {
    thread::spawn(move || {
        let outcome = load_directory_entries(&target_path).map_err(|error| DirectoryLoadError {
            message: error.to_string(),
        });

        if let Ok(mut pending) = results.lock() {
            pending.push(DirectoryLoadResult {
                generation,
                target_path,
                outcome,
            });
        }

        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window_weak.upgrade() {
                window.invoke_process_directory_load_results();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_result(generation: u64) -> DirectoryLoadResult<()> {
        DirectoryLoadResult {
            generation: LoadGeneration(generation),
            target_path: PathBuf::from("/workspace"),
            outcome: Ok(()),
        }
    }

    fn err_result(generation: u64) -> DirectoryLoadResult<()> {
        DirectoryLoadResult {
            generation: LoadGeneration(generation),
            target_path: PathBuf::from("/workspace"),
            outcome: Err(DirectoryLoadError {
                message: "load failed".to_string(),
            }),
        }
    }

    #[test]
    fn background_loader_drops_stale_success_results() {
        let action = classify_result_for_application(LoadGeneration(2), &ok_result(1));
        assert_eq!(action, LoadResultAction::DropStale);
    }

    #[test]
    fn background_loader_drops_stale_failure_results() {
        let action = classify_result_for_application(LoadGeneration(3), &err_result(2));
        assert_eq!(action, LoadResultAction::DropStale);
    }

    #[test]
    fn background_loader_applies_current_success_results() {
        let action = classify_result_for_application(LoadGeneration(4), &ok_result(4));
        assert_eq!(action, LoadResultAction::ApplySuccess);
    }

    #[test]
    fn background_loader_applies_current_failure_results() {
        let action = classify_result_for_application(LoadGeneration(5), &err_result(5));
        assert_eq!(action, LoadResultAction::ApplyFailure);
    }
}
