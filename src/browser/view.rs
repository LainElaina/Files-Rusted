use slint::SharedString;

use super::DirectoryEntry;

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
            return SharedString::from(format!(
                "{} is hidden by the current filter",
                entry.name
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
        SharedString::from(format!("{} item(s) loaded", visible_count))
    } else {
        SharedString::from(format!(
            "{} of {} item(s) match \"{}\"",
            visible_count, total_count, filter_query
        ))
    }
}
