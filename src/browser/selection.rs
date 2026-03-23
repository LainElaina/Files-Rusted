use std::{collections::HashSet, path::PathBuf};

#[derive(Clone, Default)]
pub(super) struct SelectionState {
    selected_paths: Vec<PathBuf>,
    primary_selected_path: Option<PathBuf>,
    selection_anchor_path: Option<PathBuf>,
}

impl SelectionState {
    pub(super) fn selected_paths(&self) -> &[PathBuf] {
        &self.selected_paths
    }

    pub(super) fn primary_selected_path(&self) -> Option<&PathBuf> {
        self.primary_selected_path.as_ref()
    }

    pub(super) fn selection_anchor_path(&self) -> Option<&PathBuf> {
        self.selection_anchor_path.as_ref()
    }

    pub(super) fn select_range_to(&mut self, visible: &[PathBuf], target: PathBuf, union_existing: bool) {
        let anchor = self
            .selection_anchor_path
            .clone()
            .or_else(|| self.primary_selected_path.clone())
            .unwrap_or_else(|| target.clone());

        let anchor_index = visible.iter().position(|path| *path == anchor);
        let target_index = visible.iter().position(|path| *path == target);

        let Some(anchor_index) = anchor_index else {
            self.set_single_selection(Some(target.clone()));
            return;
        };
        let Some(target_index) = target_index else {
            self.set_single_selection(Some(target.clone()));
            return;
        };

        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };

        let mut selection = visible[start..=end].to_vec();
        if union_existing {
            selection.extend(self.selected_paths.iter().cloned());
        }

        self.set_explicit_selection(selection, Some(target), Some(anchor));
    }

    pub(super) fn toggle_selection(&mut self, target: PathBuf) {
        let mut selection = self.selected_paths.clone();
        if let Some(index) = selection.iter().position(|path| *path == target) {
            selection.remove(index);
            if selection.is_empty() {
                self.set_explicit_selection(selection, None, None);
            } else {
                let fallback = selection[index.min(selection.len() - 1)].clone();
                self.set_explicit_selection(
                    selection,
                    Some(fallback.clone()),
                    Some(fallback),
                );
            }
        } else {
            selection.push(target.clone());
            self.set_explicit_selection(selection, Some(target.clone()), Some(target));
        }
    }

    pub(super) fn set_single_selection(&mut self, path: Option<PathBuf>) {
        match path {
            Some(path) => {
                self.set_explicit_selection(vec![path.clone()], Some(path.clone()), Some(path))
            }
            None => self.clear_selection(),
        }
    }

    pub(super) fn set_explicit_selection(
        &mut self,
        paths: Vec<PathBuf>,
        primary: Option<PathBuf>,
        anchor: Option<PathBuf>,
    ) {
        self.selected_paths = dedupe_paths(paths);
        self.primary_selected_path = primary;
        self.selection_anchor_path = anchor;
    }

    pub(super) fn reconcile_selection(&mut self, existing_paths: &[PathBuf]) {
        let existing_paths = existing_paths.iter().collect::<HashSet<_>>();

        let selection = self
            .selected_paths
            .iter()
            .filter(|path| existing_paths.contains(path))
            .cloned()
            .collect::<Vec<_>>();

        let primary = self
            .primary_selected_path
            .clone()
            .filter(|path| existing_paths.contains(path));
        let anchor = self
            .selection_anchor_path
            .clone()
            .filter(|path| existing_paths.contains(path));

        self.set_explicit_selection(selection, primary, anchor);
    }

    pub(super) fn ensure_selection_anchor(&mut self, anchor: Option<PathBuf>) {
        self.selection_anchor_path = anchor;
    }

    pub(super) fn clear_selection(&mut self) {
        self.selected_paths.clear();
        self.primary_selected_path.take();
        self.selection_anchor_path.take();
    }

    pub(super) fn selected_items_for_operation(&self) -> Vec<PathBuf> {
        if !self.selected_paths.is_empty() {
            return normalize_operation_paths(self.selected_paths.clone());
        }

        self.primary_selected_path
            .clone()
            .map(|path| vec![path])
            .unwrap_or_default()
    }

    pub(super) fn set_focus_only(&mut self, path: Option<PathBuf>) {
        self.primary_selected_path = path;
    }
}

pub(super) fn normalize_operation_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let unique = dedupe_paths(paths);

    let mut normalized = Vec::new();
    for path in unique {
        if normalized.iter().any(|existing: &PathBuf| path.starts_with(existing)) {
            continue;
        }

        normalized.retain(|existing| !existing.starts_with(&path));
        normalized.push(path);
    }

    normalized
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if unique.iter().any(|existing| existing == &path) {
            continue;
        }
        unique.push(path);
    }
    unique
}
