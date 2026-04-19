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
    pub(super) empty_state_title: SharedString,
    pub(super) empty_state_detail: SharedString,
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
    show_hidden: bool,
    selection: &SelectionState,
    rename_mode: bool,
    rename_draft: &str,
) -> BrowserViewData {
    let total_count = loaded_entries.len() as i32;
    let hidden_count = loaded_entries
        .iter()
        .filter(|entry| entry.is_hidden)
        .count() as i32;

    let effective_filter = filter_query.trim().to_lowercase();
    let mut visible_entries = loaded_entries
        .iter()
        .filter(|entry| {
            (show_hidden || !entry.is_hidden)
                && (effective_filter.is_empty() || entry.name_lower.contains(&effective_filter))
        })
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
    let primary_hidden_reason = primary_entry
        .and_then(|entry| hidden_reason(entry, &effective_filter, show_hidden))
        .or_else(|| {
            if primary_entry.is_some() && !primary_visible {
                Some(HiddenReason::View)
            } else {
                None
            }
        });

    let selection_text = build_selection_text(
        selected_count,
        operation_count,
        primary_entry,
        primary_visible,
        primary_hidden_reason,
    );
    let status_text = build_status_text(
        visible_count,
        total_count,
        hidden_count,
        filter_query.trim(),
        primary_entry,
        primary_visible,
        primary_hidden_reason,
        selected_count,
        operation_count,
        show_hidden,
    );
    let (empty_state_title, empty_state_detail) = build_empty_state_text(
        visible_count,
        total_count,
        hidden_count,
        filter_query.trim(),
        show_hidden,
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
        empty_state_title,
        empty_state_detail,
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
    primary_hidden_reason: Option<HiddenReason>,
) -> SharedString {
    match selected_count {
        0 => {
            if operation_count == 1 {
                if let Some(entry) = primary_entry {
                    if primary_visible {
                        return SharedString::from(format!("Focused: {}", entry.name));
                    }

                    return SharedString::from(format!(
                        "Focused: {} ({})",
                        entry.name,
                        hidden_reason_selection_suffix(
                            primary_hidden_reason.unwrap_or(HiddenReason::View)
                        )
                    ));
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
                    "Selected {}: {} ({})",
                    kind,
                    entry.name,
                    hidden_reason_selection_suffix(
                        primary_hidden_reason.unwrap_or(HiddenReason::View)
                    )
                ))
            }
        }
        count => SharedString::from(format!("{} items selected", count)),
    }
}

pub(super) fn build_status_text(
    visible_count: i32,
    total_count: i32,
    hidden_count: i32,
    filter_query: &str,
    primary_entry: Option<&DirectoryEntry>,
    primary_visible: bool,
    primary_hidden_reason: Option<HiddenReason>,
    selected_count: usize,
    _operation_count: usize,
    show_hidden: bool,
) -> SharedString {
    if selected_count > 1 {
        return SharedString::from(format!("{} items selected", selected_count));
    }

    let primary_hidden_reason = primary_hidden_reason.or_else(|| {
        primary_entry.map(|entry| {
            if !filter_query.is_empty() {
                HiddenReason::Filter
            } else if !show_hidden && entry.is_hidden {
                HiddenReason::HiddenFilesOff
            } else {
                HiddenReason::View
            }
        })
    });

    if let Some(entry) = primary_entry {
        if selected_count == 0 {
            if primary_visible {
                return SharedString::from(format!("Focused: {}", entry.name));
            }

            return SharedString::from(format!(
                "{} {}",
                entry.name,
                hidden_reason_status_suffix(primary_hidden_reason.unwrap_or(HiddenReason::View))
            ));
        }

        if !primary_visible {
            return SharedString::from(format!(
                "{} {}",
                entry.name,
                hidden_reason_status_suffix(primary_hidden_reason.unwrap_or(HiddenReason::View))
            ));
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
        if !show_hidden && hidden_count > 0 {
            return SharedString::from(format!(
                "{} item(s) loaded ({} hidden)",
                visible_count, hidden_count
            ));
        }

        SharedString::from(format!("{} item(s) loaded", visible_count))
    } else {
        SharedString::from(format!(
            "{} of {} item(s) match \"{}\"",
            visible_count, total_count, filter_query
        ))
    }
}

fn build_empty_state_text(
    visible_count: i32,
    total_count: i32,
    hidden_count: i32,
    filter_query: &str,
    show_hidden: bool,
) -> (SharedString, SharedString) {
    if visible_count > 0 {
        return (
            SharedString::from("This folder is empty"),
            SharedString::from("Use the context menu or shortcuts to create items."),
        );
    }

    if total_count == 0 {
        return (
            SharedString::from("This folder is empty"),
            SharedString::from("Use the context menu or shortcuts to create items."),
        );
    }

    if filter_query.is_empty() && !show_hidden && hidden_count > 0 {
        return (
            SharedString::from("Only hidden items are in this folder"),
            SharedString::from("Turn on Hidden or press Ctrl+H to reveal them."),
        );
    }

    if !filter_query.is_empty() {
        return (
            SharedString::from("No items match the current search"),
            SharedString::from("Adjust the search text to see more items."),
        );
    }

    (
        SharedString::from("No items match the current view"),
        SharedString::from("Change the current filters to reveal more items."),
    )
}

#[derive(Clone, Copy)]
pub(super) enum HiddenReason {
    Filter,
    HiddenFilesOff,
    View,
}

fn hidden_reason(
    entry: &DirectoryEntry,
    effective_filter: &str,
    show_hidden: bool,
) -> Option<HiddenReason> {
    let hidden_by_filter =
        !effective_filter.is_empty() && !entry.name_lower.contains(effective_filter);
    let hidden_by_hidden_files = !show_hidden && entry.is_hidden;

    match (hidden_by_filter, hidden_by_hidden_files) {
        (true, false) => Some(HiddenReason::Filter),
        (false, true) => Some(HiddenReason::HiddenFilesOff),
        (true, true) => Some(HiddenReason::View),
        (false, false) => None,
    }
}

fn hidden_reason_selection_suffix(reason: HiddenReason) -> &'static str {
    match reason {
        HiddenReason::Filter => "hidden by filter",
        HiddenReason::HiddenFilesOff => "hidden because hidden files are off",
        HiddenReason::View => "hidden by current view",
    }
}

fn hidden_reason_status_suffix(reason: HiddenReason) -> &'static str {
    match reason {
        HiddenReason::Filter => "is hidden by the current filter",
        HiddenReason::HiddenFilesOff => "is hidden because hidden files are off",
        HiddenReason::View => "is hidden by the current view",
    }
}
