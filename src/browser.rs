use slint::{ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{AppWindow, FileEntry, SidebarEntry};

#[path = "browser/drag_selection.rs"]
mod drag_selection;
#[path = "browser/file_ops.rs"]
mod file_ops;
#[path = "browser/pathing.rs"]
mod pathing;
#[path = "browser/selection.rs"]
mod selection;
#[path = "browser/view.rs"]
mod view;

use drag_selection::{
    compute_drag_autoscroll_delta, DragPoint, DragRect, DragScrollViewport, DragSelectionSession,
    DragSelectionSnapshot, VisibleItemLayout,
};
use file_ops::{
    copy_path, destination_for_transfer, format_item_count, item_name, launch_path, move_path,
    unique_child_path,
};
use pathing::{
    build_breadcrumbs, build_sidebar_entries, current_sidebar_index, load_directory_entries,
};
use selection::{normalize_operation_paths, SelectionState};

pub struct BrowserState {
    current_dir: RefCell<PathBuf>,
    loaded_entries: RefCell<Vec<DirectoryEntry>>,
    visible_paths: RefCell<Vec<PathBuf>>,
    visible_item_layouts: RefCell<Vec<VisibleItemLayout>>,
    drag_selection_session: RefCell<Option<DragSelectionSession>>,
    drag_selection_rect: RefCell<Option<DragRect>>,
    drag_scroll_viewport: RefCell<Option<DragScrollViewport>>,
    pending_drag_autoscroll: RefCell<f32>,
    drag_pointer: RefCell<Option<DragPoint>>,
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
                drag_scroll_viewport: RefCell::new(None),
                pending_drag_autoscroll: RefCell::new(0.0),
                drag_pointer: RefCell::new(None),
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

    fn derived_view_for_apply(&self) -> view::BrowserViewData {
        let filter_query = self.filter_query.borrow().clone();
        let sort_mode = *self.sort_mode.borrow();
        let rename_mode = *self.rename_mode.borrow();
        let rename_draft = self.rename_draft.borrow().clone();

        let mut derived = {
            let loaded_entries = self.loaded_entries.borrow();
            let selection_state = self.selection_state.borrow();
            view::build_browser_view(
                &loaded_entries,
                sort_mode,
                &filter_query,
                &selection_state,
                rename_mode,
                &rename_draft,
            )
        };

        if let Some(status_override) = self.status_override.borrow().clone() {
            derived.status_text = SharedString::from(status_override);
        }

        derived
    }

    fn apply_view_to_state_and_model(&self, file_model: &VecModel<FileEntry>) -> view::BrowserViewData {
        let derived = self.derived_view_for_apply();
        *self.visible_paths.borrow_mut() = derived.visible_paths.clone();
        file_model.set_vec(derived.file_rows.clone());
        derived
    }

    fn apply_view(&self, window: &AppWindow, file_model: &VecModel<FileEntry>) {
        let current_dir = self.current_dir.borrow().clone();
        let filter_query = self.filter_query.borrow().clone();
        let sort_mode = *self.sort_mode.borrow();

        let derived = self.apply_view_to_state_and_model(file_model);

        self.update_breadcrumbs(window, &current_dir);
        window.set_current_path(SharedString::from(current_dir.display().to_string()));
        window.set_item_count(derived.visible_count);
        window.set_total_item_count(derived.total_count);
        window.set_selected_file_index(derived.focused_index);
        window.set_selection_text(derived.selection_text);
        window.set_status_text(derived.status_text);
        window.set_clipboard_text(SharedString::from(self.clipboard_text()));
        window.set_can_open_selection(derived.can_open_selection);
        window.set_can_rename_selection(derived.can_rename_selection);
        window.set_can_delete_selection(derived.can_delete_selection);
        window.set_can_transfer_selection(derived.can_transfer_selection);
        window.set_can_paste(self.pending_transfer.borrow().is_some());
        window.set_rename_mode(derived.rename_mode);
        window.set_rename_draft(derived.rename_draft);
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

    pub fn clear_visible_item_layouts(&self, _window: &AppWindow, _file_model: &VecModel<FileEntry>) {
        self.replace_visible_item_layouts(Vec::new());
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

    pub fn set_drag_scroll_viewport_from_ui(
        &self,
        content_top: f32,
        content_height: f32,
        scroll_position: f32,
        max_scroll_position: f32,
    ) {
        self.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top,
            content_height,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position,
            max_scroll_position,
        }));
    }

    pub fn execute_drag_autoscroll_step_from_ui(
        &self,
        applied_delta: f32,
        window: &AppWindow,
        file_model: &VecModel<FileEntry>,
    ) {
        self.recompute_drag_selection_after_scroll_step(applied_delta);
        self.apply_view(window, file_model);
    }

    fn recompute_drag_selection_after_scroll_step(&self, applied_delta: f32) {
        if self.drag_selection_session.borrow().is_none() {
            *self.pending_drag_autoscroll.borrow_mut() = 0.0;
            return;
        }

        let pointer = *self.drag_pointer.borrow();
        if let Some(pointer) = pointer {
            self.update_drag_selection(pointer);
        }

        if applied_delta == 0.0 {
            *self.pending_drag_autoscroll.borrow_mut() = 0.0;
        }
    }

    pub fn drag_autoscroll_step_for_active_drag(&self) -> f32 {
        if self.drag_selection_session.borrow().is_none() {
            return 0.0;
        }

        let pointer = *self.drag_pointer.borrow();
        if let Some(pointer) = pointer {
            self.compute_pending_drag_autoscroll(pointer);
        }

        std::mem::take(&mut *self.pending_drag_autoscroll.borrow_mut())
    }

    fn set_drag_scroll_viewport(&self, viewport: Option<DragScrollViewport>) {
        *self.drag_scroll_viewport.borrow_mut() = viewport;
        if viewport.is_none() {
            *self.pending_drag_autoscroll.borrow_mut() = 0.0;
        }
    }

    #[cfg(test)]
    pub fn pending_drag_autoscroll(&self) -> f32 {
        *self.pending_drag_autoscroll.borrow()
    }

    fn compute_pending_drag_autoscroll(&self, pointer: DragPoint) {
        let delta = self
            .drag_scroll_viewport
            .borrow()
            .map(|viewport| {
                let requested = compute_drag_autoscroll_delta(pointer.y, viewport);
                if requested < 0.0 && viewport.scroll_position <= 0.0 {
                    0.0
                } else if requested > 0.0 && viewport.scroll_position >= viewport.max_scroll_position {
                    0.0
                } else {
                    requested
                }
            })
            .unwrap_or(0.0);
        *self.pending_drag_autoscroll.borrow_mut() = delta;
    }

    fn begin_drag_selection(&self, start: DragPoint, control: bool) {
        *self.drag_selection_session.borrow_mut() = Some(DragSelectionSession::begin(
            start,
            control,
            self.drag_snapshot(),
        ));
        self.drag_selection_rect.borrow_mut().take();
        *self.pending_drag_autoscroll.borrow_mut() = 0.0;
        *self.drag_pointer.borrow_mut() = Some(start);
    }

    fn update_drag_selection(&self, point: DragPoint) {
        let Some(session) = self.drag_selection_session.borrow().clone() else {
            return;
        };

        *self.drag_pointer.borrow_mut() = Some(point);

        let result = session.selection_for(point, &self.visible_item_layouts.borrow(), 4.0);
        *self.drag_selection_rect.borrow_mut() = result.rect;
        if self.drag_selection_rect.borrow().is_some() {
            self.compute_pending_drag_autoscroll(point);
        } else {
            *self.pending_drag_autoscroll.borrow_mut() = 0.0;
        }
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
        *self.pending_drag_autoscroll.borrow_mut() = 0.0;
        self.drag_pointer.borrow_mut().take();
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

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn build_browser_view_filters_rows_and_keeps_focus_index() {
        let entries = vec![
            directory_entry("/workspace/zeta.txt", false, 20),
            directory_entry("/workspace/alpha.txt", false, 10),
            directory_entry("/workspace/docs", true, 0),
        ];

        let mut selection = SelectionState::default();
        selection.set_focus_only(Some(PathBuf::from("/workspace/alpha.txt")));

        let view = view::build_browser_view(
            &entries,
            SortMode::NameAsc,
            "txt",
            &selection,
            false,
            "",
        );

        assert_eq!(
            view.visible_paths,
            vec![
                PathBuf::from("/workspace/alpha.txt"),
                PathBuf::from("/workspace/zeta.txt"),
            ]
        );
        assert_eq!(view.visible_count, 2);
        assert_eq!(view.total_count, 3);
        assert_eq!(view.focused_index, 0);
        assert_eq!(view.file_rows.len(), 2);
        assert_eq!(view.file_rows[0].name.as_str(), "alpha.txt");
        assert!(view.file_rows[0].focused);
        assert!(!view.file_rows[0].selected);
        assert_eq!(view.file_rows[1].name.as_str(), "zeta.txt");
        assert!(!view.file_rows[1].focused);
        assert!(!view.file_rows[1].selected);
        assert_eq!(view.selection_text.as_str(), "Focused: alpha.txt");
        assert_eq!(view.status_text.as_str(), "Focused: alpha.txt");
        assert!(view.can_delete_selection);
        assert!(view.can_transfer_selection);
        assert!(view.can_open_selection);
        assert!(view.can_rename_selection);
    }

    #[test]
    fn build_browser_view_reports_hidden_selected_item() {
        let entries = vec![
            directory_entry("/workspace/zeta.txt", false, 20),
            directory_entry("/workspace/alpha.txt", false, 10),
            directory_entry("/workspace/docs", true, 0),
        ];

        let mut selection = SelectionState::default();
        selection.set_single_selection(Some(PathBuf::from("/workspace/zeta.txt")));

        let view = view::build_browser_view(
            &entries,
            SortMode::NameAsc,
            "alp",
            &selection,
            false,
            "",
        );

        assert_eq!(
            view.visible_paths,
            vec![PathBuf::from("/workspace/alpha.txt")]
        );
        assert_eq!(view.focused_index, -1);
        assert_eq!(
            view.selection_text.as_str(),
            "Selected file: zeta.txt (hidden by filter)"
        );
        assert_eq!(
            view.status_text.as_str(),
            "zeta.txt is hidden by the current filter"
        );
        assert!(view.can_delete_selection);
        assert!(view.can_transfer_selection);
    }

    #[test]
    fn build_status_text_reports_hidden_primary_selection() {
        let entry = DirectoryEntry {
            path: PathBuf::from("/workspace/notes.txt"),
            name: "notes.txt".to_string(),
            name_lower: "notes.txt".to_string(),
            path_label: "/workspace/notes.txt".to_string(),
            kind_label: "File".to_string(),
            is_dir: false,
            size_bytes: 12,
            size_label: "12 B".to_string(),
        };

        let text = view::build_status_text(0, 3, "abc", Some(&entry), false, 1, 1);

        assert_eq!(text.as_str(), "notes.txt is hidden by the current filter");
    }

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
    fn browser_state_drag_update_does_not_request_autoscroll_before_threshold_even_in_hot_zones() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        state.replace_visible_item_layouts(layouts);
        state.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top: 0.0,
            content_height: 300.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position: 120.0,
            max_scroll_position: 240.0,
        }));

        state.begin_drag_selection(DragPoint::new(100.0, 2.0), false);
        state.update_drag_selection(DragPoint::new(102.0, 3.0));
        assert_eq!(state.pending_drag_autoscroll(), 0.0);

        state.begin_drag_selection(DragPoint::new(100.0, 298.0), false);
        state.update_drag_selection(DragPoint::new(102.0, 297.0));
        assert_eq!(state.pending_drag_autoscroll(), 0.0);
    }

    #[test]
    fn browser_state_drag_update_requests_autoscroll_near_bottom() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        state.replace_visible_item_layouts(layouts);
        state.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top: 0.0,
            content_height: 300.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position: 120.0,
            max_scroll_position: 240.0,
        }));

        state.begin_drag_selection(DragPoint::new(0.0, 0.0), false);
        state.update_drag_selection(DragPoint::new(280.0, 295.0));

        assert!(state.pending_drag_autoscroll() > 0.0);
    }

    #[test]
    fn browser_state_drag_autoscroll_preserves_ctrl_drag_baseline_behavior() {
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
        state.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top: 0.0,
            content_height: 300.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position: 120.0,
            max_scroll_position: 240.0,
        }));

        state.begin_drag_selection(DragPoint::new(0.0, 120.0), true);
        state.update_drag_selection(DragPoint::new(280.0, 295.0));

        let requested = state.drag_autoscroll_step_for_active_drag();
        assert!(requested > 0.0);

        state.finish_drag_selection();

        assert_eq!(
            state.selection_state.borrow().selected_paths(),
            [path("a.txt"), path("b.txt")]
        );
        assert_eq!(state.pending_drag_autoscroll(), 0.0);
    }

    #[test]
    fn browser_state_drag_autoscroll_tick_keeps_running_without_pointer_moves() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        state.replace_visible_item_layouts(layouts);
        state.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top: 0.0,
            content_height: 300.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position: 120.0,
            max_scroll_position: 240.0,
        }));

        state.begin_drag_selection(DragPoint::new(0.0, 120.0), false);
        state.update_drag_selection(DragPoint::new(280.0, 295.0));
        assert!(state.drag_autoscroll_step_for_active_drag() > 0.0);

        let tick_delta = state.drag_autoscroll_step_for_active_drag();
        assert!(tick_delta > 0.0);
    }

    #[test]
    fn browser_state_drag_autoscroll_step_recomputes_selection_after_scroll() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts_before = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];
        let layouts_after = vec![
            layout("a.txt", 0.0, -120.0, 300.0, 84.0),
            layout("b.txt", 0.0, -28.0, 300.0, 84.0),
            layout("c.txt", 0.0, 64.0, 300.0, 84.0),
        ];

        state.replace_visible_item_layouts(layouts_before);
        state.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top: 0.0,
            content_height: 300.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position: 120.0,
            max_scroll_position: 240.0,
        }));

        state.begin_drag_selection(DragPoint::new(0.0, 0.0), false);
        state.update_drag_selection(DragPoint::new(280.0, 150.0));
        assert_eq!(
            state.selection_state.borrow().selected_paths(),
            [path("a.txt"), path("b.txt")]
        );

        state.replace_visible_item_layouts(layouts_after);
        state.recompute_drag_selection_after_scroll_step(24.0);

        assert_eq!(
            state.selection_state.borrow().selected_paths(),
            [path("b.txt"), path("c.txt")]
        );
    }

    #[test]
    fn browser_state_drag_autoscroll_stops_at_boundaries() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        state.replace_visible_item_layouts(layouts);

        state.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top: 0.0,
            content_height: 300.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position: 0.0,
            max_scroll_position: 240.0,
        }));
        state.begin_drag_selection(DragPoint::new(0.0, 120.0), false);
        state.update_drag_selection(DragPoint::new(280.0, 4.0));
        assert_eq!(state.drag_autoscroll_step_for_active_drag(), 0.0);

        state.set_drag_scroll_viewport(Some(DragScrollViewport {
            content_top: 0.0,
            content_height: 300.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
            scroll_position: 240.0,
            max_scroll_position: 240.0,
        }));
        state.update_drag_selection(DragPoint::new(280.0, 296.0));
        assert_eq!(state.drag_autoscroll_step_for_active_drag(), 0.0);
    }

    #[test]
    fn browser_state_apply_view_uses_extracted_view_builder() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let file_model = VecModel::from(Vec::<FileEntry>::new());

        state.loaded_entries.borrow_mut().extend([
            directory_entry("/workspace/alpha.txt", false, 10),
            directory_entry("/workspace/beta.txt", false, 20),
        ]);
        *state.filter_query.borrow_mut() = "alpha".to_string();
        state
            .selection_state
            .borrow_mut()
            .set_single_selection(Some(PathBuf::from("/workspace/beta.txt")));

        let view = state.apply_view_to_state_and_model(&file_model);

        assert_eq!(
            state.visible_paths.borrow().clone(),
            vec![PathBuf::from("/workspace/alpha.txt")]
        );
        assert_eq!(file_model.row_count(), 1);
        let row = file_model.row_data(0).expect("visible row");
        assert_eq!(row.name.as_str(), "alpha.txt");
        assert_eq!(
            view.selection_text.as_str(),
            "Selected file: beta.txt (hidden by filter)"
        );
        assert_eq!(
            view.status_text.as_str(),
            "beta.txt is hidden by the current filter"
        );
    }

    #[test]
    fn browser_state_public_selection_flow_still_works_after_refactor() {
        let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
        let layouts = vec![
            layout("a.txt", 0.0, 0.0, 300.0, 84.0),
            layout("b.txt", 0.0, 92.0, 300.0, 84.0),
            layout("c.txt", 0.0, 184.0, 300.0, 84.0),
        ];

        state
            .visible_paths
            .borrow_mut()
            .extend([path("a.txt"), path("b.txt"), path("c.txt")]);
        state.replace_visible_item_layouts(layouts);

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

    fn directory_entry(path: &str, is_dir: bool, size_bytes: u64) -> DirectoryEntry {
        let path = PathBuf::from(path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        DirectoryEntry {
            path: path.clone(),
            name: name.clone(),
            name_lower: name.to_lowercase(),
            path_label: path.display().to_string(),
            kind_label: if is_dir {
                "Folder".to_string()
            } else {
                "File".to_string()
            },
            is_dir,
            size_bytes,
            size_label: format!("{} B", size_bytes),
        }
    }
}
