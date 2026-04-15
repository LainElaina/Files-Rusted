use slint::SharedString;
use std::{collections::HashSet, path::PathBuf};

use crate::FileEntry;

use super::{selection::SelectionState, DirectoryEntry, SortMode};

pub(super) struct BrowserViewData {
    pub(super) file_rows: Vec<FileEntry>,
    pub(super) visible_paths: Vec<PathBuf>,
    pub(super) visible_count: i32,
    pub(super) total_count: i32,
    pub(super) focused_index: i32,
    pub(super) selection_text: SharedString,
    pub(super) status_text: SharedString,
    pub(super) can_open_selection: bool,
    pub(super) can_rename_selection: bool,
    pub(super) can_delete_selection: bool,
    pub(super) can_transfer_selection: bool,
    pub(super) rename_mode: bool,
    pub(super) rename_draft: SharedString,
}

pub(super) fn build_browser_view(
    loaded_entries: &[DirectoryEntry],
    sort_mode: SortMode,
    filter_query: &str,
    selection: &SelectionState,
    rename_mode: bool,
    rename_draft: &str,
) -> BrowserViewData {
    let total_count = loaded_entries.len() as i32;

    let effective_filter = filter_query.trim().to_lowercase();
    let mut visible_entries = loaded_entries
        .iter()
        .filter(|entry| effective_filter.is_empty() || entry.name_lower.contains(&effective_filter))
        .cloned()
        .collect::<Vec<_>>();
    visible_entries.sort_by(|left, right| sort_mode.compare(left, right));

    let visible_paths = visible_entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let visible_count = visible_paths.len() as i32;

    let primary_selected = selection.primary_selected_path().cloned();
    let selected_paths = selection.selected_paths().to_vec();
    let selected_lookup = selected_paths.iter().cloned().collect::<HashSet<_>>();
    let selected_count = selected_paths.len();
    let operation_paths = selection.selected_items_for_operation();
    let operation_count = operation_paths.len();

    let focused_index = primary_selected
        .as_ref()
        .and_then(|path| visible_paths.iter().position(|candidate| candidate == path))
        .map(|index| index as i32)
        .unwrap_or(-1);
    let primary_visible = focused_index >= 0;
    let primary_entry = primary_selected
        .as_ref()
        .and_then(|path| loaded_entries.iter().find(|entry| entry.path == *path));

    let selection_text = build_selection_text(
        selected_count,
        operation_count,
        primary_entry,
        primary_visible,
    );
    let status_text = build_status_text(
        visible_count,
        total_count,
        filter_query.trim(),
        primary_entry,
        primary_visible,
        selected_count,
        operation_count,
    );

    let file_rows = visible_entries
        .into_iter()
        .map(|entry| {
            let selected = selected_lookup.contains(&entry.path);
            let focused = primary_selected
                .as_ref()
                .is_some_and(|path| path == &entry.path);
            entry.into_view(selected, focused)
        })
        .collect::<Vec<_>>();

    BrowserViewData {
        file_rows,
        visible_paths,
        visible_count,
        total_count,
        focused_index,
        selection_text,
        status_text,
        can_open_selection: operation_count > 0,
        can_rename_selection: operation_count == 1,
        can_delete_selection: operation_count > 0,
        can_transfer_selection: operation_count > 0,
        rename_mode: rename_mode && operation_count == 1,
        rename_draft: SharedString::from(rename_draft),
    }
}

pub(super) fn build_selection_text(
    selected_count: usize,
    operation_count: usize,
    primary_entry: Option<&DirectoryEntry>,
    primary_visible: bool,
) -> SharedString {
    match selected_count {
        0 => {
            if operation_count == 1 {
                if let Some(entry) = primary_entry {
                    return SharedString::from(format!("Focused: {}", entry.name));
                }
            }

            SharedString::from("No item selected")
        }
        1 => {
            let Some(entry) = primary_entry else {
                return SharedString::from("1 item selected");
            };

            let kind = if entry.is_dir { "folder" } else { "file" };
            if primary_visible {
                SharedString::from(format!("Selected {}: {}", kind, entry.name))
            } else {
                SharedString::from(format!(
                    "Selected {}: {} (hidden by filter)",
                    kind, entry.name
                ))
            }
        }
        count => SharedString::from(format!("{} items selected", count)),
    }
}

pub(super) fn build_status_text(
    visible_count: i32,
    total_count: i32,
    filter_query: &str,
    primary_entry: Option<&DirectoryEntry>,
    primary_visible: bool,
    selected_count: usize,
    _operation_count: usize,
) -> SharedString {
    if selected_count > 1 {
        return SharedString::from(format!("{} items selected", selected_count));
    }

    if let Some(entry) = primary_entry {
        if selected_count == 0 {
            return SharedString::from(format!("Focused: {}", entry.name));
        }

        if !primary_visible {
            return SharedString::from(format!("{} is hidden by the current filter", entry.name));
        }

        if entry.is_dir {
            return SharedString::from(format!(
                "Folder {} selected. Double-click or use Open",
                entry.name
            ));
        }

        return SharedString::from(format!("File {} selected", entry.name));
    }

    if filter_query.is_empty() {
        SharedString::from(format!("{} item(s) loaded", visible_count))
    } else {
        SharedString::from(format!(
            "{} of {} item(s) match \"{}\"",
            visible_count, total_count, filter_query
        ))
    }
}
