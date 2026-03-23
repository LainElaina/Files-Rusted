use slint::{ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
};

use crate::{AppWindow, BreadcrumbEntry, FileEntry, SidebarEntry};

#[path = "browser/selection.rs"]
mod selection;
#[path = "browser/drag_selection.rs"]
mod drag_selection;

use drag_selection::{
    DragPoint, DragRect, DragSelectionSession, DragSelectionSnapshot, VisibleItemLayout,
};
use selection::{normalize_operation_paths, SelectionState};

pub struct BrowserState {
    current_dir: RefCell<PathBuf>,
    loaded_entries: RefCell<Vec<DirectoryEntry>>,
    visible_paths: RefCell<Vec<PathBuf>>,
    visible_item_layouts: RefCell<Vec<VisibleItemLayout>>,
    drag_selection_session: RefCell<Option<DragSelectionSession>>,
    drag_selection_rect: RefCell<Option<DragRect>>,
    selection_state: RefCell<SelectionState>,
    sort_mode: RefCell<SortMode>,
    filter_query: RefCell<String>,
    sidebar_paths: Vec<PathBuf>,
    breadcrumb_paths: RefCell<Vec<PathBuf>>,
    back_history: RefCell<Vec<PathBuf>>,
    forward_history: RefCell<Vec<PathBuf>>,
    rename_mode: RefCell<bool>,
    rename_draft: RefCell<String>,
    pending_transfer: RefCell<Option<TransferOperation>>,
    status_override: RefCell<Option<String>>,
}

impl BrowserState {
    pub fn new(start_dir: PathBuf) -> (Self, Vec<SidebarEntry>) {
        let (sidebar_entries, sidebar_paths) = build_sidebar_entries(&start_dir);
        (
            Self {
                current_dir: RefCell::new(start_dir),
                loaded_entries: RefCell::new(Vec::new()),
                visible_paths: RefCell::new(Vec::new()),
                visible_item_layouts: RefCell::new(Vec::new()),
                drag_selection_session: RefCell::new(None),
                drag_selection_rect: RefCell::new(None),
                selection_state: RefCell::new(SelectionState::default()),
                sort_mode: RefCell::new(SortMode::NameAsc),
                filter_query: RefCell::new(String::new()),
                sidebar_paths,
                breadcrumb_paths: RefCell::new(Vec::new()),
                back_history: RefCell::new(Vec::new()),
                forward_history: RefCell::new(Vec::new()),
                rename_mode: RefCell::new(false),
                rename_draft: RefCell::new(String::new()),
                pending_transfer: RefCell::new(None),
                status_override: RefCell::new(None),
            },
            sidebar_entries,
        )
    }

    pub fn sort_options() -> Vec<SharedString> {
        SortMode::ALL
            .iter()
            .map(|mode| SharedString::from(mode.label()))
            .collect()
    }

    pub fn current_sort_index(&self) -> i32 {
        self.sort_mode.borrow().index()
    }

    pub fn refresh(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let current_dir = self.current_dir.borrow().clone();

        match load_directory_entries(&current_dir) {
            Ok(entries) => {
                *self.loaded_entries.borrow_mut() = entries;
                self.apply_view(window, file_model);
            }
            Err(error) => {
                self.loaded_entries.borrow_mut().clear();
                self.visible_paths.borrow_mut().clear();
                self.selection_state.borrow_mut().clear_selection();
                file_model.set_vec(Vec::new());

                self.update_breadcrumbs(window, &current_dir);
                window.set_current_path(SharedString::from(current_dir.display().to_string()));
                window.set_item_count(0);
                window.set_total_item_count(0);
                window.set_selected_file_index(-1);
                window.set_selection_text(SharedString::from("No item selected"));
                window.set_status_text(SharedString::from(format!(
                    "Failed to open directory: {}",
                    error
                )));
                window.set_can_open_selection(false);
                window.set_can_rename_selection(false);
                window.set_can_delete_selection(false);
                window.set_can_transfer_selection(false);
                window.set_can_paste(false);
                window.set_rename_mode(false);
                window.set_rename_draft(SharedString::from(""));
                window.set_clipboard_text(SharedString::from(self.clipboard_text()));
                window.set_active_sidebar_index(current_sidebar_index(
                    &self.sidebar_paths,
                    &current_dir,
                ));
                window.set_can_navigate_back(!self.back_history.borrow().is_empty());
                window.set_can_navigate_forward(!self.forward_history.borrow().is_empty());
            }
        }
    }

    pub fn navigate_home(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        if let Some(target) = self.sidebar_paths.first().cloned() {
            self.navigate_to(target, NavigationMode::PushCurrent, window, file_model);
        }
    }

    pub fn navigate_up(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let parent = self.current_dir.borrow().parent().map(Path::to_path_buf);
        if let Some(parent) = parent {
            self.navigate_to(parent, NavigationMode::PushCurrent, window, file_model);
        }
    }

    pub fn navigate_back(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let Some(target) = self.back_history.borrow_mut().pop() else {
            return;
        };

        let current = self.current_dir.borrow().clone();
        self.forward_history.borrow_mut().push(current);
        self.navigate_to(target, NavigationMode::History, window, file_model);
    }

    pub fn navigate_forward(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let Some(target) = self.forward_history.borrow_mut().pop() else {
            return;
        };

        let current = self.current_dir.borrow().clone();
        self.back_history.borrow_mut().push(current);
        self.navigate_to(target, NavigationMode::History, window, file_model);
    }

    pub fn activate_sidebar(
        &self,
        index: i32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let index = index.max(0) as usize;
        if let Some(target) = self.sidebar_paths.get(index).cloned() {
            self.navigate_to(target, NavigationMode::PushCurrent, window, file_model);
        }
    }

    pub fn activate_breadcrumb(
        &self,
        index: i32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let index = index.max(0) as usize;
        if let Some(target) = self.breadcrumb_paths.borrow().get(index).cloned() {
            self.navigate_to(target, NavigationMode::PushCurrent, window, file_model);
        }
    }

    pub fn activate_file(
        &self,
        index: i32,
        control: bool,
        shift: bool,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        self.activate_file_selection(index, control, shift);
        self.apply_view(window, file_model);
    }

    fn activate_file_selection(&self, index: i32, control: bool, shift: bool) {
        let Some(target) = self.path_at_visible_index(index) else {
            return;
        };

        self.clear_status_override();
        self.cancel_rename_internal();

        if shift {
            self.selection_state
                .borrow_mut()
                .select_range_to(&self.visible_paths.borrow(), target.clone(), control);
        } else if control {
            self.selection_state
                .borrow_mut()
                .toggle_selection(target.clone());
        } else {
            self.selection_state
                .borrow_mut()
                .set_single_selection(Some(target.clone()));
        }

        if !control || shift {
            self.selection_state
                .borrow_mut()
                .ensure_selection_anchor(Some(target));
        }
    }


    pub fn open_selected(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let selected_paths = self.selection_state.borrow().selected_items_for_operation();
        if selected_paths.is_empty() {
            *self.status_override.borrow_mut() = Some("Nothing selected to open".to_string());
            self.apply_view(window, file_model);
            return;
        }

        if selected_paths.len() == 1 {
            self.open_path(selected_paths[0].clone(), window, file_model);
            return;
        }

        if selected_paths.iter().any(|path| path.is_dir()) {
            *self.status_override.borrow_mut() = Some(
                "Open with multiple selection currently supports files only".to_string(),
            );
            self.apply_view(window, file_model);
            return;
        }

        let mut opened = 0usize;
        let mut failed = 0usize;

        for path in &selected_paths {
            match launch_path(path) {
                Ok(()) => opened += 1,
                Err(_) => failed += 1,
            }
        }

        *self.status_override.borrow_mut() = Some(if failed == 0 {
            format!("Opening {} file(s)", opened)
        } else {
            format!("Opening {} file(s), {} failed", opened, failed)
        });
        self.apply_view(window, file_model);
    }

    pub fn open_item(&self, index: i32, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let Some(target) = self.path_at_visible_index(index) else {
            return;
        };

        self.selection_state
            .borrow_mut()
            .set_single_selection(Some(target.clone()));
        self.selection_state
            .borrow_mut()
            .ensure_selection_anchor(Some(target.clone()));
        self.open_path(target, window, file_model);
    }

    pub fn create_file(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let current_dir = self.current_dir.borrow().clone();
        let target = unique_child_path(&current_dir, "New File", Some("txt"));

        match fs::File::create(&target) {
            Ok(_) => {
                self.cancel_rename_internal();
                self.selection_state
                    .borrow_mut()
                    .set_single_selection(Some(target.clone()));
                *self.status_override.borrow_mut() =
                    Some(format!("Created file {}", item_name(&target)));
                self.refresh(window, file_model);
                self.request_rename_selected(window, file_model);
            }
            Err(error) => {
                *self.status_override.borrow_mut() =
                    Some(format!("Failed to create file: {}", error));
                self.apply_view(window, file_model);
            }
        }
    }

    pub fn create_folder(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let current_dir = self.current_dir.borrow().clone();
        let target = unique_child_path(&current_dir, "New Folder", None);

        match fs::create_dir(&target) {
            Ok(()) => {
                self.cancel_rename_internal();
                self.selection_state
                    .borrow_mut()
                    .set_single_selection(Some(target.clone()));
                *self.status_override.borrow_mut() =
                    Some(format!("Created folder {}", item_name(&target)));
                self.refresh(window, file_model);
                self.request_rename_selected(window, file_model);
            }
            Err(error) => {
                *self.status_override.borrow_mut() =
                    Some(format!("Failed to create folder: {}", error));
                self.apply_view(window, file_model);
            }
        }
    }

    pub fn request_rename_selected(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let selection = self.selection_state.borrow().selected_items_for_operation();
        if selection.len() != 1 {
            *self.status_override.borrow_mut() =
                Some("Rename requires exactly one selected item".to_string());
            self.apply_view(window, file_model);
            return;
        }

        let selected = selection.first().cloned();
        let Some(path) = selected else {
            *self.status_override.borrow_mut() = Some("Nothing selected to rename".to_string());
            self.apply_view(window, file_model);
            return;
        };

        *self.rename_mode.borrow_mut() = true;
        *self.rename_draft.borrow_mut() = item_name(&path);
        self.clear_status_override();
        self.apply_view(window, file_model);
    }

    pub fn request_rename_item(
        &self,
        index: i32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        if let Some(target) = self.path_at_visible_index(index) {
            self.selection_state
                .borrow_mut()
                .set_single_selection(Some(target));
            self.request_rename_selected(window, file_model);
        }
    }

    pub fn set_rename_draft(&self, value: String) {
        *self.rename_draft.borrow_mut() = value;
    }

    pub fn request_copy_selected(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let selected = self.selection_state.borrow().selected_items_for_operation();
        if selected.is_empty() {
            *self.status_override.borrow_mut() = Some("Select an item before copying".to_string());
            self.apply_view(window, file_model);
            return;
        }

        self.cancel_rename_internal();
        *self.pending_transfer.borrow_mut() = Some(TransferOperation {
            kind: TransferKind::Copy,
            sources: selected.clone(),
        });
        *self.status_override.borrow_mut() =
            Some(format!("Ready to copy {}", format_item_count(selected.len())));
        self.apply_view(window, file_model);
    }

    pub fn request_cut_selected(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let selected = self.selection_state.borrow().selected_items_for_operation();
        if selected.is_empty() {
            *self.status_override.borrow_mut() = Some("Select an item before cutting".to_string());
            self.apply_view(window, file_model);
            return;
        }

        self.cancel_rename_internal();
        *self.pending_transfer.borrow_mut() = Some(TransferOperation {
            kind: TransferKind::Cut,
            sources: selected.clone(),
        });
        *self.status_override.borrow_mut() =
            Some(format!("Ready to move {}", format_item_count(selected.len())));
        self.apply_view(window, file_model);
    }

    pub fn request_copy_item(
        &self,
        index: i32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        if let Some(target) = self.path_at_visible_index(index) {
            self.selection_state
                .borrow_mut()
                .set_single_selection(Some(target));
            self.request_copy_selected(window, file_model);
        }
    }

    pub fn request_cut_item(
        &self,
        index: i32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        if let Some(target) = self.path_at_visible_index(index) {
            self.selection_state
                .borrow_mut()
                .set_single_selection(Some(target));
            self.request_cut_selected(window, file_model);
        }
    }

    pub fn paste_into_current_dir(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let transfer = self.pending_transfer.borrow().clone();
        let Some(transfer) = transfer else {
            *self.status_override.borrow_mut() = Some("Clipboard is empty".to_string());
            self.apply_view(window, file_model);
            return;
        };

        let current_dir = self.current_dir.borrow().clone();
        let mut pasted_paths = Vec::new();
        let mut failures = Vec::new();

        for source in &transfer.sources {
            if !source.exists() {
                failures.push(format!("{} no longer exists", item_name(source)));
                continue;
            }

            if source.is_dir() && current_dir.starts_with(source) {
                failures.push(format!(
                    "Cannot paste folder {} into itself",
                    item_name(source)
                ));
                continue;
            }

            if matches!(transfer.kind, TransferKind::Cut)
                && source.parent().is_some_and(|parent| parent == current_dir)
            {
                failures.push(format!("{} is already in this directory", item_name(source)));
                continue;
            }

            let destination = destination_for_transfer(transfer.kind, source, &current_dir);
            let operation_result = match transfer.kind {
                TransferKind::Copy => copy_path(source, &destination),
                TransferKind::Cut => move_path(source, &destination),
            };

            match operation_result {
                Ok(()) => pasted_paths.push(destination),
                Err(error) => failures.push(format!("{}: {}", item_name(source), error)),
            }
        }

        if matches!(transfer.kind, TransferKind::Cut) {
            let remaining = transfer
                .sources
                .into_iter()
                .filter(|source| source.exists())
                .collect::<Vec<_>>();

            if remaining.is_empty() {
                self.pending_transfer.borrow_mut().take();
            } else {
                *self.pending_transfer.borrow_mut() = Some(TransferOperation {
                    kind: TransferKind::Cut,
                    sources: remaining,
                });
            }
        }

        if pasted_paths.is_empty() {
            *self.status_override.borrow_mut() = Some(if failures.is_empty() {
                "Nothing was pasted".to_string()
            } else {
                format!("Paste failed: {}", failures.join("; "))
            });
            self.apply_view(window, file_model);
            return;
        }

        self.cancel_rename_internal();
        self.selection_state.borrow_mut().set_explicit_selection(
            pasted_paths.clone(),
            pasted_paths.last().cloned(),
            pasted_paths.last().cloned(),
        );

        *self.status_override.borrow_mut() = Some(if failures.is_empty() {
            format!(
                "{} pasted into {}",
                format_item_count(pasted_paths.len()),
                current_dir.display()
            )
        } else {
            format!(
                "{} pasted, {} issue(s)",
                format_item_count(pasted_paths.len()),
                failures.len()
            )
        });

        self.refresh(window, file_model);
    }

    pub fn commit_rename(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let selection = self.selection_state.borrow().selected_items_for_operation();
        if selection.len() != 1 {
            *self.status_override.borrow_mut() =
                Some("Rename requires exactly one selected item".to_string());
            self.cancel_rename_internal();
            self.apply_view(window, file_model);
            return;
        }

        let selected = selection.first().cloned();
        let Some(path) = selected else {
            *self.status_override.borrow_mut() = Some("Nothing selected to rename".to_string());
            self.cancel_rename_internal();
            self.apply_view(window, file_model);
            return;
        };

        let draft = self.rename_draft.borrow().trim().to_string();
        if draft.is_empty() {
            *self.status_override.borrow_mut() = Some("Name cannot be empty".to_string());
            self.apply_view(window, file_model);
            return;
        }

        let current_name = item_name(&path);
        if draft == current_name {
            self.cancel_rename_internal();
            *self.status_override.borrow_mut() = Some(format!("Name unchanged: {}", current_name));
            self.apply_view(window, file_model);
            return;
        }

        let Some(parent) = path.parent().map(Path::to_path_buf) else {
            *self.status_override.borrow_mut() =
                Some("Cannot rename the filesystem root".to_string());
            self.apply_view(window, file_model);
            return;
        };

        let new_path = parent.join(&draft);
        if new_path.exists() {
            *self.status_override.borrow_mut() =
                Some(format!("An item named {} already exists", draft));
            self.apply_view(window, file_model);
            return;
        }

        match fs::rename(&path, &new_path) {
            Ok(()) => {
                self.cancel_rename_internal();
                self.selection_state
                    .borrow_mut()
                    .set_single_selection(Some(new_path.clone()));
                *self.status_override.borrow_mut() =
                    Some(format!("Renamed to {}", item_name(&new_path)));
                self.refresh(window, file_model);
            }
            Err(error) => {
                *self.status_override.borrow_mut() = Some(format!("Rename failed: {}", error));
                self.apply_view(window, file_model);
            }
        }
    }

    pub fn cancel_rename(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        self.cancel_rename_internal();
        self.clear_status_override();
        self.apply_view(window, file_model);
    }

    pub fn delete_selected(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let selected = self.selection_state.borrow().selected_items_for_operation();
        if selected.is_empty() {
            *self.status_override.borrow_mut() = Some("Nothing selected to delete".to_string());
            self.apply_view(window, file_model);
            return;
        }

        self.delete_paths(selected, window, file_model);
    }

    pub fn delete_item(
        &self,
        index: i32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        if let Some(target) = self.path_at_visible_index(index) {
            self.delete_paths(vec![target], window, file_model);
        }
    }

    pub fn set_sort_mode(
        &self,
        index: i32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        *self.sort_mode.borrow_mut() = SortMode::from_index(index);
        self.apply_view(window, file_model);
    }

    pub fn set_filter_query(
        &self,
        query: String,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        *self.filter_query.borrow_mut() = query;
        self.clear_status_override();
        self.apply_view(window, file_model);
    }

    pub fn move_focus_next(
        &self,
        extend: bool,
        control: bool,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        self.move_selection(1, extend, control, window, file_model);
    }

    pub fn move_focus_previous(
        &self,
        extend: bool,
        control: bool,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        self.move_selection(-1, extend, control, window, file_model);
    }

    pub fn move_focus_to_boundary(
        &self,
        to_end: bool,
        extend: bool,
        control: bool,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let visible = self.visible_paths.borrow().clone();
        if visible.is_empty() {
            return;
        }

        let target = if to_end {
            visible.last().cloned()
        } else {
            visible.first().cloned()
        };
        let Some(target) = target else {
            return;
        };

        self.clear_status_override();
        self.cancel_rename_internal();

        if extend {
            self.selection_state
                .borrow_mut()
                .select_range_to(&self.visible_paths.borrow(), target.clone(), control);
        } else if control {
            self.selection_state.borrow_mut().set_focus_only(Some(target.clone()));
        } else {
            self.selection_state
                .borrow_mut()
                .set_single_selection(Some(target.clone()));
        }

        if !control || extend {
            self.selection_state
            .borrow_mut()
            .ensure_selection_anchor(Some(target));
        }
        self.apply_view(window, file_model);
    }

    pub fn select_all(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let visible_paths = self.visible_paths.borrow().clone();
        if visible_paths.is_empty() {
            return;
        }

        let primary = self
            .selection_state
            .borrow()
            .primary_selected_path()
            .filter(|path| visible_paths.contains(path))
            .cloned()
            .or_else(|| visible_paths.first().cloned());
        let anchor = visible_paths.first().cloned();

        self.cancel_rename_internal();
        self.clear_status_override();
        self.selection_state
            .borrow_mut()
            .set_explicit_selection(visible_paths, primary, anchor);
        self.apply_view(window, file_model);
    }

    pub fn clear_selection_command(
        &self,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        self.clear_status_override();
        self.cancel_rename_internal();
        self.selection_state.borrow_mut().clear_selection();
        self.apply_view(window, file_model);
    }

    pub fn toggle_focused_selection(
        &self,
        extend: bool,
        control: bool,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let focused = self.selection_state.borrow().primary_selected_path().cloned();
        let Some(target) = focused else {
            return;
        };

        self.clear_status_override();
        self.cancel_rename_internal();

        if extend {
            self.selection_state
                .borrow_mut()
                .select_range_to(&self.visible_paths.borrow(), target.clone(), control);
        } else if control {
            self.selection_state.borrow_mut().toggle_selection(target);
        } else {
            self.selection_state
                .borrow_mut()
                .set_single_selection(Some(target.clone()));
            self.selection_state
            .borrow_mut()
            .ensure_selection_anchor(Some(target));
        }

        self.apply_view(window, file_model);
    }

    fn navigate_to(
        &self,
        target: PathBuf,
        mode: NavigationMode,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let current = self.current_dir.borrow().clone();
        if current == target {
            if matches!(mode, NavigationMode::History) {
                self.refresh(window, file_model);
            }
            return;
        }

        if matches!(mode, NavigationMode::PushCurrent) {
            self.back_history.borrow_mut().push(current);
            self.forward_history.borrow_mut().clear();
        }

        self.clear_status_override();
        self.cancel_rename_internal();
        self.selection_state.borrow_mut().clear_selection();
        *self.current_dir.borrow_mut() = target;
        self.refresh(window, file_model);
    }

    fn apply_view(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let current_dir = self.current_dir.borrow().clone();
        let filter_query = self.filter_query.borrow().clone();
        let sort_mode = *self.sort_mode.borrow();
        let loaded_entries = self.loaded_entries.borrow();
        let total_count = loaded_entries.len() as i32;

        let loaded_paths = loaded_entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        self.selection_state
            .borrow_mut()
            .reconcile_selection(&loaded_paths);

        let effective_filter = filter_query.trim().to_lowercase();
        let mut visible_entries = loaded_entries
            .iter()
            .filter(|entry| {
                effective_filter.is_empty() || entry.name_lower.contains(&effective_filter)
            })
            .cloned()
            .collect::<Vec<_>>();

        visible_entries.sort_by(|left, right| sort_mode.compare(left, right));

        let visible_paths = visible_entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let visible_count = visible_paths.len() as i32;

        let primary_selected = self.selection_state.borrow().primary_selected_path().cloned();
        let selected_paths = self.selection_state.borrow().selected_paths().to_vec();
        let selected_lookup = selected_paths.iter().cloned().collect::<std::collections::HashSet<_>>();
        let selected_count = selected_paths.len();
        let operation_paths = self.selection_state.borrow().selected_items_for_operation();
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
        let rename_mode = *self.rename_mode.borrow();
        let rename_draft = self.rename_draft.borrow().clone();

        let selection_text = build_selection_text(
            selected_count,
            operation_count,
            primary_entry,
            primary_visible,
        );
        let status_text = self
            .status_override
            .borrow()
            .clone()
            .unwrap_or_else(|| {
                build_status_text(
                    visible_count,
                    total_count,
                    filter_query.trim(),
                    primary_entry,
                    primary_visible,
                    selected_count,
                    operation_count,
                )
                .to_string()
            });

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

        *self.visible_paths.borrow_mut() = visible_paths;
        file_model.set_vec(file_rows);

        self.update_breadcrumbs(window, &current_dir);
        window.set_current_path(SharedString::from(current_dir.display().to_string()));
        window.set_item_count(visible_count);
        window.set_total_item_count(total_count);
        window.set_selected_file_index(focused_index);
        window.set_selection_text(selection_text);
        window.set_status_text(SharedString::from(status_text));
        window.set_clipboard_text(SharedString::from(self.clipboard_text()));
        window.set_can_open_selection(operation_count > 0);
        window.set_can_rename_selection(operation_count == 1);
        window.set_can_delete_selection(operation_count > 0);
        window.set_can_transfer_selection(operation_count > 0);
        window.set_can_paste(self.pending_transfer.borrow().is_some());
        window.set_rename_mode(rename_mode && operation_count == 1);
        window.set_rename_draft(SharedString::from(rename_draft));
        if let Some(rect) = *self.drag_selection_rect.borrow() {
            window.set_drag_selection_active(true);
            window.set_drag_selection_x(rect.x);
            window.set_drag_selection_y(rect.y);
            window.set_drag_selection_width(rect.width);
            window.set_drag_selection_height(rect.height);
        } else {
            window.set_drag_selection_active(false);
            window.set_drag_selection_x(0.0);
            window.set_drag_selection_y(0.0);
            window.set_drag_selection_width(0.0);
            window.set_drag_selection_height(0.0);
        }
        window.set_active_sidebar_index(current_sidebar_index(
            &self.sidebar_paths,
            &current_dir,
        ));
        window.set_filter_text(SharedString::from(filter_query));
        window.set_current_sort_index(sort_mode.index());
        window.set_can_navigate_back(!self.back_history.borrow().is_empty());
        window.set_can_navigate_forward(!self.forward_history.borrow().is_empty());
    }

    fn drag_snapshot(&self) -> DragSelectionSnapshot {
        DragSelectionSnapshot {
            selected: self.selection_state.borrow().selected_paths().to_vec(),
            primary: self.selection_state.borrow().primary_selected_path().cloned(),
            anchor: self.selection_state.borrow().selection_anchor_path().cloned(),
        }
    }

    fn replace_visible_item_layouts(&self, layouts: Vec<VisibleItemLayout>) {
        *self.visible_item_layouts.borrow_mut() = layouts;
    }

    pub fn clear_visible_item_layouts(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        self.replace_visible_item_layouts(Vec::new());
        self.apply_view(window, file_model);
    }

    pub fn register_visible_item_layout(
        &self,
        index: i32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let Some(path) = self.path_at_visible_index(index) else {
            return;
        };

        let mut layouts = self.visible_item_layouts.borrow().clone();
        let rect = DragRect::new(x, y, width, height);
        if let Some(existing) = layouts.iter_mut().find(|layout| layout.path == path) {
            existing.rect = rect;
        } else {
            layouts.push(VisibleItemLayout { path, rect });
        }
        self.replace_visible_item_layouts(layouts);
        if self.drag_selection_rect.borrow().is_some() {
            self.apply_view(window, file_model);
        }
    }

    pub fn begin_drag_selection_from_ui(
        &self,
        x: f32,
        y: f32,
        control: bool,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        self.clear_status_override();
        self.cancel_rename_internal();
        self.begin_drag_selection(DragPoint::new(x, y), control);
        self.apply_view(window, file_model);
    }

    pub fn update_drag_selection_from_ui(
        &self,
        x: f32,
        y: f32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        self.update_drag_selection(DragPoint::new(x, y));
        self.apply_view(window, file_model);
    }

    pub fn finish_drag_selection_from_ui(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        self.finish_drag_selection();
        self.apply_view(window, file_model);
    }

    pub fn has_active_drag_selection(&self) -> bool {
        self.drag_selection_session.borrow().is_some()
    }

    fn begin_drag_selection(&self, start: DragPoint, control: bool) {
        *self.drag_selection_session.borrow_mut() = Some(DragSelectionSession::begin(
            start,
            control,
            self.drag_snapshot(),
        ));
        self.drag_selection_rect.borrow_mut().take();
    }

    fn update_drag_selection(&self, point: DragPoint) {
        let Some(session) = self.drag_selection_session.borrow().clone() else {
            return;
        };

        let result = session.selection_for(point, &self.visible_item_layouts.borrow(), 4.0);
        *self.drag_selection_rect.borrow_mut() = result.rect;
        self.selection_state
            .borrow_mut()
            .set_explicit_selection(result.selected, result.primary, result.anchor);
    }

    fn finish_drag_selection(&self) {
        let session = self.drag_selection_session.borrow().clone();
        let Some(session) = session else {
            return;
        };

        if self.drag_selection_rect.borrow().is_none() {
            if session.control {
                self.selection_state.borrow_mut().set_explicit_selection(
                    session.baseline.selected,
                    session.baseline.primary,
                    session.baseline.anchor,
                );
            } else {
                self.selection_state.borrow_mut().clear_selection();
            }
        }

        self.drag_selection_session.borrow_mut().take();
        self.drag_selection_rect.borrow_mut().take();
    }

    fn update_breadcrumbs(&self, window: &AppWindow, current_dir: &Path) {
        let (items, paths) = build_breadcrumbs(current_dir);
        *self.breadcrumb_paths.borrow_mut() = paths;
        window.set_breadcrumb_items(ModelRc::from(Rc::new(VecModel::from(items))));
    }

    fn open_path(&self, path: PathBuf, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        self.clear_status_override();
        self.cancel_rename_internal();

        if path.is_dir() {
            self.navigate_to(path, NavigationMode::PushCurrent, window, file_model);
            return;
        }

        match launch_path(&path) {
            Ok(()) => {
                *self.status_override.borrow_mut() =
                    Some(format!("Opening {}", item_name(&path)));
            }
            Err(error) => {
                *self.status_override.borrow_mut() =
                    Some(format!("Open failed: {}", error));
            }
        }

        self.apply_view(window, file_model);
    }

    fn delete_paths(
        &self,
        paths: Vec<PathBuf>,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let selected = normalize_operation_paths(paths);
        if selected.is_empty() {
            *self.status_override.borrow_mut() = Some("Nothing selected to delete".to_string());
            self.apply_view(window, file_model);
            return;
        }

        let mut deleted = 0usize;
        let mut failures = 0usize;

        for path in &selected {
            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };

            match result {
                Ok(()) => {
                    deleted += 1;
                    self.clear_transfer_if_matches(path);
                }
                Err(_) => failures += 1,
            }
        }

        self.cancel_rename_internal();
        self.selection_state.borrow_mut().clear_selection();
        *self.status_override.borrow_mut() = Some(if failures == 0 {
            format!("Deleted {}", format_item_count(deleted))
        } else {
            format!("Deleted {}, {} failed", format_item_count(deleted), failures)
        });
        self.refresh(window, file_model);
    }

    fn move_selection(
        &self,
        delta: i32,
        extend: bool,
        control: bool,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        let visible = self.visible_paths.borrow().clone();
        if visible.is_empty() {
            return;
        }

        let current_index = self
            .selection_state
            .borrow()
            .primary_selected_path()
            .and_then(|path| visible.iter().position(|candidate| candidate == path))
            .map(|index| index as i32)
            .unwrap_or(if delta >= 0 { -1 } else { visible.len() as i32 });

        let next_index = (current_index + delta).clamp(0, visible.len() as i32 - 1) as usize;
        let target = visible[next_index].clone();

        self.clear_status_override();
        self.cancel_rename_internal();

        if extend {
            self.selection_state
                .borrow_mut()
                .select_range_to(&self.visible_paths.borrow(), target.clone(), control);
        } else if control {
            self.selection_state.borrow_mut().set_focus_only(Some(target.clone()));
        } else {
            self.selection_state
                .borrow_mut()
                .set_single_selection(Some(target.clone()));
        }

        if !control || extend {
            self.selection_state
            .borrow_mut()
            .ensure_selection_anchor(Some(target));
        }
        self.apply_view(window, file_model);
    }

    fn cancel_rename_internal(&self) {
        *self.rename_mode.borrow_mut() = false;
        self.rename_draft.borrow_mut().clear();
    }

    fn clipboard_text(&self) -> String {
        match self.pending_transfer.borrow().as_ref() {
            Some(transfer) => match transfer.kind {
                TransferKind::Copy => format!("Clipboard: copy {}", format_item_count(transfer.sources.len())),
                TransferKind::Cut => format!("Clipboard: move {}", format_item_count(transfer.sources.len())),
            },
            None => "Clipboard: empty".to_string(),
        }
    }

    fn clear_transfer_if_matches(&self, path: &Path) {
        let mut transfer = self.pending_transfer.borrow().clone();
        let Some(current) = transfer.as_mut() else {
            return;
        };

        current.sources.retain(|source| source != path);
        if current.sources.is_empty() {
            self.pending_transfer.borrow_mut().take();
        } else {
            *self.pending_transfer.borrow_mut() = transfer;
        }
    }

    fn clear_status_override(&self) {
        self.status_override.borrow_mut().take();
    }

    fn path_at_visible_index(&self, index: i32) -> Option<PathBuf> {
        let index = index.max(0) as usize;
        self.visible_paths.borrow().get(index).cloned()
    }
}

#[derive(Clone)]
struct DirectoryEntry {
    path: PathBuf,
    name: String,
    name_lower: String,
    path_label: String,
    kind_label: String,
    is_dir: bool,
    size_bytes: u64,
    size_label: String,
}

impl DirectoryEntry {
    fn into_view(self, selected: bool, focused: bool) -> FileEntry {
        FileEntry {
            name: SharedString::from(self.name),
            path: SharedString::from(self.path_label),
            kind: SharedString::from(self.kind_label),
            size: SharedString::from(self.size_label),
            selected,
            focused,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortMode {
    NameAsc,
    NameDesc,
    SizeAsc,
    SizeDesc,
}

impl SortMode {
    const ALL: [Self; 4] = [
        Self::NameAsc,
        Self::NameDesc,
        Self::SizeAsc,
        Self::SizeDesc,
    ];

    fn from_index(index: i32) -> Self {
        match index {
            1 => Self::NameDesc,
            2 => Self::SizeAsc,
            3 => Self::SizeDesc,
            _ => Self::NameAsc,
        }
    }

    fn index(self) -> i32 {
        match self {
            Self::NameAsc => 0,
            Self::NameDesc => 1,
            Self::SizeAsc => 2,
            Self::SizeDesc => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::NameAsc => "Name A-Z",
            Self::NameDesc => "Name Z-A",
            Self::SizeAsc => "Size Small-Large",
            Self::SizeDesc => "Size Large-Small",
        }
    }

    fn compare(self, left: &DirectoryEntry, right: &DirectoryEntry) -> std::cmp::Ordering {
        right.is_dir.cmp(&left.is_dir).then_with(|| match self {
            Self::NameAsc => left.name_lower.cmp(&right.name_lower),
            Self::NameDesc => right.name_lower.cmp(&left.name_lower),
            Self::SizeAsc => left
                .size_bytes
                .cmp(&right.size_bytes)
                .then_with(|| left.name_lower.cmp(&right.name_lower)),
            Self::SizeDesc => right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.name_lower.cmp(&right.name_lower)),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NavigationMode {
    PushCurrent,
    History,
}

#[derive(Clone)]
struct TransferOperation {
    kind: TransferKind,
    sources: Vec<PathBuf>,
}

#[derive(Clone, Copy)]
enum TransferKind {
    Copy,
    Cut,
}

fn build_sidebar_entries(start_dir: &Path) -> (Vec<SidebarEntry>, Vec<PathBuf>) {
    let mut entries = Vec::new();
    let mut paths = Vec::new();

    if let Some(home) = home_directory() {
        push_sidebar_entry(&mut entries, &mut paths, "Home", home);
    }

    push_sidebar_entry(
        &mut entries,
        &mut paths,
        "Workspace",
        start_dir.to_path_buf(),
    );
    push_sidebar_entry(&mut entries, &mut paths, "Root", filesystem_root(start_dir));

    (entries, paths)
}

fn push_sidebar_entry(
    entries: &mut Vec<SidebarEntry>,
    paths: &mut Vec<PathBuf>,
    label: &str,
    path: PathBuf,
) {
    if paths.iter().any(|existing| existing == &path) {
        return;
    }

    entries.push(SidebarEntry {
        label: SharedString::from(label),
        caption: SharedString::from(short_path_label(&path)),
    });
    paths.push(path);
}

fn build_breadcrumbs(current_dir: &Path) -> (Vec<BreadcrumbEntry>, Vec<PathBuf>) {
    let mut paths = current_dir
        .ancestors()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    paths.reverse();

    let items = paths
        .iter()
        .map(|path| BreadcrumbEntry {
            label: SharedString::from(breadcrumb_label(path)),
        })
        .collect::<Vec<_>>();

    (items, paths)
}

fn breadcrumb_label(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            let text = path.display().to_string();
            if text.is_empty() {
                "/".to_string()
            } else {
                text
            }
        })
}

fn load_directory_entries(path: &Path) -> Result<Vec<DirectoryEntry>, std::io::Error> {
    let entries = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.is_empty() {
                return None;
            }

            let is_dir = metadata.is_dir();
            let size_bytes = if is_dir { 0 } else { metadata.len() };

            Some(DirectoryEntry {
                path_label: path.display().to_string(),
                kind_label: if is_dir {
                    "Folder".to_string()
                } else {
                    file_kind_label(&path)
                },
                size_label: if is_dir {
                    "—".to_string()
                } else {
                    format_bytes(size_bytes)
                },
                name_lower: name.to_lowercase(),
                path,
                name,
                is_dir,
                size_bytes,
            })
        })
        .collect::<Vec<_>>();

    Ok(entries)
}

fn build_selection_text(
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

fn build_status_text(
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

fn current_sidebar_index(sidebar_paths: &[PathBuf], current_dir: &Path) -> i32 {
    sidebar_paths
        .iter()
        .enumerate()
        .filter(|(_, path)| current_dir.starts_with(path))
        .max_by_key(|(_, path)| path.components().count())
        .map(|(index, _)| index as i32)
        .unwrap_or(0)
}

fn home_directory() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

fn filesystem_root(path: &Path) -> PathBuf {
    let mut root = path.to_path_buf();

    while let Some(parent) = root.parent() {
        root = parent.to_path_buf();
    }

    root
}

fn short_path_label(path: &Path) -> String {
    let text = path.display().to_string();
    let chars = text.chars().collect::<Vec<_>>();

    if chars.len() <= 30 {
        text
    } else {
        let suffix = chars[chars.len() - 29..].iter().collect::<String>();
        format!("…{suffix}")
    }
}

fn file_kind_label(path: &Path) -> String {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty());

    match extension {
        Some(extension) => format!("{} file", extension.to_uppercase()),
        None => "File".to_string(),
    }
}

fn item_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn unique_child_path(parent: &Path, base_name: &str, extension: Option<&str>) -> PathBuf {
    let extension = extension
        .map(str::to_string)
        .filter(|value| !value.is_empty());
    let mut index = 1;

    loop {
        let candidate_name = if index == 1 {
            base_name.to_string()
        } else {
            format!("{} {}", base_name, index)
        };

        let candidate = match extension.as_ref() {
            Some(extension) => parent.join(format!("{}.{}", candidate_name, extension)),
            None => parent.join(candidate_name),
        };

        if !candidate.exists() {
            return candidate;
        }

        index += 1;
    }
}

fn copy_path(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let child_source = entry.path();
            let child_destination = destination.join(entry.file_name());
            copy_path(&child_source, &child_destination)?;
        }

        Ok(())
    } else {
        fs::copy(source, destination).map(|_| ())
    }
}

fn move_path(source: &Path, destination: &Path) -> std::io::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_path(source, destination)?;
            if source.is_dir() {
                fs::remove_dir_all(source)
            } else {
                fs::remove_file(source)
            }
        }
    }
}

fn destination_for_transfer(kind: TransferKind, source: &Path, target_dir: &Path) -> PathBuf {
    let source_name = item_name(source);
    let original_target = target_dir.join(&source_name);

    if matches!(kind, TransferKind::Cut) && !original_target.exists() {
        return original_target;
    }

    if matches!(kind, TransferKind::Copy)
        && !original_target.exists()
        && source.parent() != Some(target_dir)
    {
        return original_target;
    }

    let (stem, extension) = split_name_parts(source);
    let copy_base = if matches!(kind, TransferKind::Copy) {
        format!("{} Copy", stem)
    } else {
        stem
    };

    unique_child_path(target_dir, &copy_base, extension.as_deref())
}

fn split_name_parts(path: &Path) -> (String, Option<String>) {
    if path.is_dir() {
        return (item_name(path), None);
    }

    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty());

    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| item_name(path));

    (stem, extension)
}

fn launch_path(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(path);
        return spawn_detached(command);
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(path);
        return spawn_detached(command);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        return spawn_detached(command);
    }

    #[allow(unreachable_code)]
    Err(std::io::Error::other(
        "Open is not implemented for this platform",
    ))
}

fn spawn_detached(mut command: Command) -> std::io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn format_item_count(count: usize) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{} items", count)
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_selection_replaces_selection_without_modifiers() {
        let base = drag_snapshot(
            vec![path("keep.txt")],
            Some(path("keep.txt")),
            Some(path("keep.txt")),
        );
        let session = DragSelectionSession::begin(DragPoint::new(0.0, 0.0), false, base);
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        let result = session.selection_for(DragPoint::new(280.0, 150.0), &layouts, 4.0);

        assert_eq!(result.selected, vec![path("a.txt"), path("b.txt")]);
        assert_eq!(result.primary, Some(path("b.txt")));
        assert_eq!(result.anchor, Some(path("b.txt")));
    }

    #[test]
    fn control_drag_toggles_against_baseline_selection() {
        let base = drag_snapshot(
            vec![path("a.txt"), path("c.txt")],
            Some(path("c.txt")),
            Some(path("c.txt")),
        );
        let session = DragSelectionSession::begin(DragPoint::new(0.0, 0.0), true, base);
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        let result = session.selection_for(DragPoint::new(280.0, 150.0), &layouts, 4.0);

        assert_eq!(result.selected, vec![path("b.txt"), path("c.txt")]);
        assert_eq!(result.primary, Some(path("b.txt")));
        assert_eq!(result.anchor, Some(path("b.txt")));
    }

    #[test]
    fn drag_selection_clears_selection_when_rectangle_hits_nothing() {
        let base = drag_snapshot(
            vec![path("a.txt")],
            Some(path("a.txt")),
            Some(path("a.txt")),
        );
        let session = DragSelectionSession::begin(DragPoint::new(0.0, 300.0), false, base);
        let layouts = vec![layout("a.txt", 0.0, 0.0, 300.0, 84.0)];

        let result = session.selection_for(DragPoint::new(20.0, 360.0), &layouts, 4.0);

        assert!(result.selected.is_empty());
        assert_eq!(result.primary, None);
        assert_eq!(result.anchor, None);
    }

    #[test]
    fn browser_state_finish_drag_promotes_selection_focus_and_anchor() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        state.replace_visible_item_layouts(layouts.clone());
        state.begin_drag_selection(DragPoint::new(0.0, 0.0), false);
        state.update_drag_selection(DragPoint::new(280.0, 150.0));
        state.finish_drag_selection();

        assert_eq!(
            state.selection_state.borrow().selected_paths(),
            [path("a.txt"), path("b.txt")]
        );
        assert_eq!(
            state.selection_state.borrow().primary_selected_path().cloned(),
            Some(path("b.txt"))
        );
        assert_eq!(
            state.selection_state.borrow().selection_anchor_path().cloned(),
            Some(path("b.txt"))
        );
        assert!(state.drag_selection_session.borrow().is_none());
    }

    #[test]
    fn browser_state_plain_workspace_click_without_drag_clears_selection() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));

        state.selection_state.borrow_mut().set_explicit_selection(
            vec![path("a.txt")],
            Some(path("a.txt")),
            Some(path("a.txt")),
        );
        state.begin_drag_selection(DragPoint::new(10.0, 10.0), false);
        state.finish_drag_selection();

        assert!(state.selection_state.borrow().selected_paths().is_empty());
        assert_eq!(state.selection_state.borrow().primary_selected_path().cloned(), None);
        assert_eq!(state.selection_state.borrow().selection_anchor_path().cloned(), None);
    }

    #[test]
    fn control_drag_keeps_unhit_baseline_items_selected() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        state.selection_state.borrow_mut().set_explicit_selection(
            vec![path("a.txt"), path("c.txt")],
            Some(path("c.txt")),
            Some(path("c.txt")),
        );
        state.replace_visible_item_layouts(layouts);
        state.begin_drag_selection(DragPoint::new(0.0, 0.0), true);
        state.update_drag_selection(DragPoint::new(280.0, 150.0));
        state.finish_drag_selection();

        assert_eq!(
            state.selection_state.borrow().selected_paths(),
            [path("b.txt"), path("c.txt")]
        );
    }

    #[test]
    fn browser_state_ctrl_toggle_removal_from_multi_selection_preserves_selected_anchor() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));

        state.visible_paths.borrow_mut().extend([path("a.txt"), path("b.txt")]);
        state.selection_state.borrow_mut().set_explicit_selection(
            vec![path("a.txt"), path("b.txt")],
            Some(path("b.txt")),
            Some(path("b.txt")),
        );

        state.activate_file_selection(1, true, false);

        assert_eq!(state.selection_state.borrow().selected_paths(), [path("a.txt")]);
        assert!(!state
            .selection_state
            .borrow()
            .selected_paths()
            .contains(&path("b.txt")));
        assert_eq!(
            state.selection_state.borrow().primary_selected_path().cloned(),
            Some(path("a.txt"))
        );
        assert_eq!(
            state.selection_state.borrow().selection_anchor_path().cloned(),
            Some(path("a.txt"))
        );
    }

    #[test]
    fn selection_state_ctrl_toggle_last_selected_item_via_activate_file_clears_anchor() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));

        state.visible_paths.borrow_mut().push(path("a.txt"));

        state.activate_file_selection(0, false, false);
        state.activate_file_selection(0, true, false);

        assert!(state.selection_state.borrow().selected_paths().is_empty());
        assert_eq!(state.selection_state.borrow().primary_selected_path().cloned(), None);
        assert_eq!(state.selection_state.borrow().selection_anchor_path().cloned(), None);
    }

    #[test]
    fn selection_state_ctrl_toggle_last_selected_item_clears_operation_selection() {
        let mut selection = SelectionState::default();

        selection.set_single_selection(Some(path("a.txt")));
        selection.toggle_selection(path("a.txt"));

        assert!(selection.selected_paths().is_empty());
        assert!(selection.selected_items_for_operation().is_empty());
    }

    #[test]
    fn selection_state_ctrl_toggle_removal_from_multi_selection_keeps_focus_on_selected_item() {
        let mut selection = SelectionState::default();

        selection.set_explicit_selection(
            vec![path("a.txt"), path("b.txt")],
            Some(path("b.txt")),
            Some(path("b.txt")),
        );
        selection.toggle_selection(path("b.txt"));

        assert_eq!(selection.selected_paths(), [path("a.txt")]);
        assert_eq!(selection.primary_selected_path().cloned(), Some(path("a.txt")));
        assert_eq!(selection.selection_anchor_path().cloned(), Some(path("a.txt")));
    }

    #[test]
    fn selection_state_keeps_ctrl_workspace_click_behavior() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));

        state.selection_state.borrow_mut().set_explicit_selection(
            vec![path("a.txt")],
            Some(path("a.txt")),
            Some(path("a.txt")),
        );
        state.begin_drag_selection(DragPoint::new(10.0, 10.0), true);
        state.finish_drag_selection();

        assert!(state.selection_state.borrow().selected_paths() == [path("a.txt")]);
        assert_eq!(
            state.selection_state.borrow().primary_selected_path().cloned(),
            Some(path("a.txt"))
        );
        assert_eq!(
            state.selection_state.borrow().selection_anchor_path().cloned(),
            Some(path("a.txt"))
        );
    }

    fn path(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    fn layout(name: &str, x: f32, y: f32, width: f32, height: f32) -> VisibleItemLayout {
        VisibleItemLayout {
            path: path(name),
            rect: DragRect::new(x, y, width, height),
        }
    }

    fn drag_snapshot(
        selected: Vec<PathBuf>,
        primary: Option<PathBuf>,
        anchor: Option<PathBuf>,
    ) -> DragSelectionSnapshot {
        DragSelectionSnapshot {
            selected,
            primary,
            anchor,
        }
    }
}
