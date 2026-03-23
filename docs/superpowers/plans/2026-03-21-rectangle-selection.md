# Rectangle Selection Implementation Plan

> 状态说明：这份 plan 对应的矩形框选任务已完成，保留它是为了记录当时的实施拆分与验收思路；当前代码状态已经继续演进，包含了后续的 selection 边界整理和 drag-autoscroll 支持，因此不要再把本文当作“待执行任务单”直接照抄。

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-pass desktop-style rectangle selection to the file list without breaking the existing click, keyboard, context-menu, and focus/selection semantics.

**Architecture:** Keep pointer input and overlay rendering in Slint while keeping hit-testing, selection merging, focus/anchor updates, and drag session state in Rust. Implement the drag-selection logic as a clearly bounded sub-structure inside `src/browser.rs` first, then wire it through `src/main.rs` and `ui/app-window.slint` without broad refactors.

**Tech Stack:** Rust 2021, Slint 1.15.1, `cargo test`, `cargo build`, `xvfb-run`

---

## Preconditions

- Spec: `docs/superpowers/specs/2026-03-21-rectangle-selection-design.md`
- Current project directory is **not** a git repository in this container, so commit steps are written as conditional follow-ups. If the project is later initialized under git, use the suggested commit commands; otherwise skip those steps.
- Keep changes focused to:
  - `src/browser.rs`
  - `src/main.rs`
  - `ui/app-window.slint`
- Do **not** introduce tabs, address bar editing, transfer engine work, or broad browser-state refactors in this plan.

## File Map

- `src/browser.rs`
  - Add drag-selection geometry/session types.
  - Add pure helper logic for hit-testing and selection merging.
  - Extend `BrowserState` with drag-session state and view-sync helpers.
  - Add `#[cfg(test)]` tests for drag-selection semantics.
- `src/main.rs`
  - Wire new Slint callbacks for drag start/update/end and per-item layout registration.
  - Keep all logic in `BrowserState`; do not add business rules here.
- `ui/app-window.slint`
  - Add drag-overlay properties/callbacks.
  - Report pointer events from the workspace list region.
  - Report visible item rectangles.
  - Preserve existing item click/double-click/right-click behavior.

## Chunk 1: Rust drag-selection core

### Task 1: Add pure drag-selection helpers and tests

**Files:**
- Modify: `src/browser.rs:1119-1648`
- Test: `src/browser.rs` (`#[cfg(test)]` module near file end)

- [ ] **Step 1: Write the failing tests**

Add a new `#[cfg(test)]` module to `src/browser.rs` with focused tests for rectangle hit-testing and selection merging. Start with these exact tests:

```rust
#[test]
fn drag_selection_replaces_selection_without_modifiers() {
    let base = drag_snapshot(vec![path("keep.txt")], Some(path("keep.txt")), Some(path("keep.txt")));
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
    let base = drag_snapshot(vec![path("a.txt")], Some(path("a.txt")), Some(path("a.txt")));
    let session = DragSelectionSession::begin(DragPoint::new(0.0, 300.0), false, base);
    let layouts = vec![layout("a.txt", 0.0, 0.0, 300.0, 84.0)];

    let result = session.selection_for(DragPoint::new(20.0, 360.0), &layouts, 4.0);

    assert!(result.selected.is_empty());
    assert_eq!(result.primary, None);
    assert_eq!(result.anchor, None);
}
```

Also add test helpers in the same module:

```rust
fn path(name: &str) -> PathBuf {
    PathBuf::from(name)
}

fn layout(name: &str, x: f32, y: f32, width: f32, height: f32) -> VisibleItemLayout {
    VisibleItemLayout {
        path: path(name),
        rect: DragRect::new(x, y, width, height),
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test drag_selection_replaces_selection_without_modifiers -- --exact
cargo test control_drag_toggles_against_baseline_selection -- --exact
cargo test drag_selection_clears_selection_when_rectangle_hits_nothing -- --exact
```

Expected: FAIL with errors about missing `DragSelectionSession`, `DragPoint`, `VisibleItemLayout`, `DragRect`, or `selection_for`.

- [ ] **Step 3: Write the minimal implementation**

Add the new pure types and helper logic near the internal structs in `src/browser.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
struct DragPoint {
    x: f32,
    y: f32,
}

impl DragPoint {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DragRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl DragRect {
    fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    fn from_points(start: DragPoint, end: DragPoint) -> Self {
        let left = start.x.min(end.x);
        let top = start.y.min(end.y);
        let right = start.x.max(end.x);
        let bottom = start.y.max(end.y);
        Self::new(left, top, right - left, bottom - top)
    }

    fn intersects(self, other: Self) -> bool {
        let self_right = self.x + self.width;
        let self_bottom = self.y + self.height;
        let other_right = other.x + other.width;
        let other_bottom = other.y + other.height;

        self.x < other_right
            && self_right > other.x
            && self.y < other_bottom
            && self_bottom > other.y
    }
}

#[derive(Clone, Debug, PartialEq)]
struct VisibleItemLayout {
    path: PathBuf,
    rect: DragRect,
}

#[derive(Clone, Debug)]
struct DragSelectionSnapshot {
    selected: Vec<PathBuf>,
    primary: Option<PathBuf>,
    anchor: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
struct DragSelectionResult {
    selected: Vec<PathBuf>,
    primary: Option<PathBuf>,
    anchor: Option<PathBuf>,
    rect: Option<DragRect>,
    active: bool,
}

#[derive(Clone, Debug)]
struct DragSelectionSession {
    start: DragPoint,
    control: bool,
    baseline: DragSelectionSnapshot,
}
```

Implement `DragSelectionSession::begin`, `DragSelectionSession::selection_for`, and a helper that:

- applies the drag threshold before marking the session active,
- builds a drag rectangle with `DragRect::from_points`,
- collects intersecting visible paths in list order,
- replaces selection for plain drag,
- toggles each hit path against the baseline selection for `Ctrl` drag,
- sets `primary` and `anchor` to the last hit path when the result is non-empty.

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
cargo test drag_selection_replaces_selection_without_modifiers -- --exact
cargo test control_drag_toggles_against_baseline_selection -- --exact
cargo test drag_selection_clears_selection_when_rectangle_hits_nothing -- --exact
```

Expected: PASS for all three tests.

- [ ] **Step 5: Commit**

If the project is under git, run:

```bash
git add src/browser.rs
git commit -m "$(cat <<'EOF'
feat: add rectangle selection core helpers

Add pure drag-selection geometry and selection-merging logic with unit tests so the list interaction can evolve without moving selection rules into Slint.
EOF
)"
```

If git is still unavailable, skip this step and continue.

### Task 2: Integrate drag-session state into `BrowserState`

**Files:**
- Modify: `src/browser.rs:12-1117`
- Modify: `src/browser.rs:1119-1648`
- Test: `src/browser.rs` (`#[cfg(test)]` module)

- [ ] **Step 1: Write the failing tests**

Add two more tests that exercise `BrowserState` directly inside `src/browser.rs`:

```rust
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

    assert_eq!(*state.selected_paths.borrow(), vec![path("a.txt"), path("b.txt")]);
    assert_eq!(*state.primary_selected_path.borrow(), Some(path("b.txt")));
    assert_eq!(*state.selection_anchor_path.borrow(), Some(path("b.txt")));
    assert!(state.drag_selection_session.borrow().is_none());
}

#[test]
fn browser_state_plain_workspace_click_without_drag_clears_selection() {
    let (state, _) = BrowserState::new(PathBuf::from("/workspace"));

    state.set_explicit_selection(
        vec![path("a.txt")],
        Some(path("a.txt")),
        Some(path("a.txt")),
    );
    state.begin_drag_selection(DragPoint::new(10.0, 10.0), false);
    state.finish_drag_selection();

    assert!(state.selected_paths.borrow().is_empty());
    assert_eq!(*state.primary_selected_path.borrow(), None);
    assert_eq!(*state.selection_anchor_path.borrow(), None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test browser_state_finish_drag_promotes_selection_focus_and_anchor -- --exact
cargo test browser_state_plain_workspace_click_without_drag_clears_selection -- --exact
```

Expected: FAIL with missing `replace_visible_item_layouts`, `begin_drag_selection`, `update_drag_selection`, `finish_drag_selection`, or missing drag-session storage on `BrowserState`.

- [ ] **Step 3: Write the minimal implementation**

Extend `BrowserState` with bounded drag-selection state:

```rust
pub struct BrowserState {
    current_dir: RefCell<PathBuf>,
    loaded_entries: RefCell<Vec<DirectoryEntry>>,
    visible_paths: RefCell<Vec<PathBuf>>,
    visible_item_layouts: RefCell<Vec<VisibleItemLayout>>,
    primary_selected_path: RefCell<Option<PathBuf>>,
    selected_paths: RefCell<Vec<PathBuf>>,
    selection_anchor_path: RefCell<Option<PathBuf>>,
    drag_selection_session: RefCell<Option<DragSelectionSession>>,
    drag_selection_rect: RefCell<Option<DragRect>>,
    // existing fields continue here
}
```

Add internal methods on `BrowserState` that do **not** call the window directly:

```rust
fn replace_visible_item_layouts(&self, layouts: Vec<VisibleItemLayout>) {
    *self.visible_item_layouts.borrow_mut() = layouts;
}

fn drag_snapshot(&self) -> DragSelectionSnapshot {
    DragSelectionSnapshot {
        selected: self.selected_paths.borrow().clone(),
        primary: self.primary_selected_path.borrow().clone(),
        anchor: self.selection_anchor_path.borrow().clone(),
    }
}

fn begin_drag_selection(&self, start: DragPoint, control: bool) {
    *self.drag_selection_session.borrow_mut() = Some(DragSelectionSession::begin(start, control, self.drag_snapshot()));
    self.drag_selection_rect.borrow_mut().take();
}

fn update_drag_selection(&self, point: DragPoint) {
    let Some(session) = self.drag_selection_session.borrow().clone() else { return; };
    let result = session.selection_for(point, &self.visible_item_layouts.borrow(), 4.0);
    *self.drag_selection_rect.borrow_mut() = result.rect;
    self.set_explicit_selection(result.selected, result.primary, result.anchor);
}

fn finish_drag_selection(&self) {
    let Some(session) = self.drag_selection_session.borrow().clone() else { return; };

    if self.drag_selection_rect.borrow().is_none() {
        self.clear_selection();
    }

    self.drag_selection_session.borrow_mut().take();
    self.drag_selection_rect.borrow_mut().take();
}
```

Then update `apply_view` so it keeps the drag overlay properties in sync after every state change.

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
cargo test browser_state_finish_drag_promotes_selection_focus_and_anchor -- --exact
cargo test browser_state_plain_workspace_click_without_drag_clears_selection -- --exact
cargo test drag_selection_replaces_selection_without_modifiers -- --exact
cargo test control_drag_toggles_against_baseline_selection -- --exact
cargo test drag_selection_clears_selection_when_rectangle_hits_nothing -- --exact
```

Expected: PASS for all targeted tests.

- [ ] **Step 5: Commit**

If the project is under git, run:

```bash
git add src/browser.rs
git commit -m "$(cat <<'EOF'
feat: track rectangle selection state in browser

Store drag-selection session state in BrowserState and promote drag results into the existing focus and anchor model without broad refactors.
EOF
)"
```

If git is still unavailable, skip this step and continue.

## Chunk 2: Slint/UI plumbing and verification

### Task 3: Wire Slint callbacks, item layout reporting, and overlay rendering

**Files:**
- Modify: `src/main.rs:8-375`
- Modify: `ui/app-window.slint:86-924`
- Modify: `src/browser.rs:12-1117`
- Test: `src/browser.rs` (`#[cfg(test)]` regression tests)

- [ ] **Step 1: Write the failing regression test**

Add one more browser-state regression test before wiring the UI:

```rust
#[test]
fn control_drag_keeps_unhit_baseline_items_selected() {
    let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
    let layouts = vec![
        layout("a.txt", 0.0, 0.0, 300.0, 84.0),
        layout("b.txt", 0.0, 92.0, 300.0, 84.0),
        layout("c.txt", 0.0, 184.0, 300.0, 84.0),
    ];

    state.set_explicit_selection(
        vec![path("a.txt"), path("c.txt")],
        Some(path("c.txt")),
        Some(path("c.txt")),
    );
    state.replace_visible_item_layouts(layouts);
    state.begin_drag_selection(DragPoint::new(0.0, 0.0), true);
    state.update_drag_selection(DragPoint::new(280.0, 150.0));
    state.finish_drag_selection();

    assert_eq!(*state.selected_paths.borrow(), vec![path("b.txt"), path("c.txt")]);
}
```

This test protects the UI wiring from accidentally reintroducing “clear everything first” behavior during `Ctrl` drag.

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test control_drag_keeps_unhit_baseline_items_selected -- --exact
```

Expected: FAIL until the in-place drag update/finalize flow is fully wired through `BrowserState`.

- [ ] **Step 3: Write the minimal implementation**

Update `ui/app-window.slint` to expose drag overlay state and pointer/layout callbacks. Add properties like:

```slint
in-out property<bool> drag-selection-active: false;
in-out property<float> drag-selection-x: 0;
in-out property<float> drag-selection-y: 0;
in-out property<float> drag-selection-width: 0;
in-out property<float> drag-selection-height: 0;

callback begin-drag-selection(float, float, bool);
callback update-drag-selection(float, float);
callback finish-drag-selection();
callback register-visible-item-layout(int, float, float, float, float);
callback clear-visible-item-layouts();
```

Then wire the list area so it:

- clears and re-registers visible item layouts whenever the visible list changes,
- starts drag selection only from non-item pointer starts,
- keeps the existing `FileCard` callbacks for click/double-click/right-click,
- draws the overlay rectangle with the drag-selection properties,
- stops treating a completed drag as a plain empty-space click.

Update `src/main.rs` with callback wiring shaped like:

```rust
window.on_begin_drag_selection(move |x, y, control| {
    if let Some(window) = window_weak.upgrade() {
        state.begin_drag_selection_from_ui(x, y, control, &window, file_model.as_ref());
    }
});

window.on_update_drag_selection(move |x, y| {
    if let Some(window) = window_weak.upgrade() {
        state.update_drag_selection_from_ui(x, y, &window, file_model.as_ref());
    }
});

window.on_finish_drag_selection(move || {
    if let Some(window) = window_weak.upgrade() {
        state.finish_drag_selection_from_ui(&window, file_model.as_ref());
    }
});
```

Finally, add matching public wrapper methods in `src/browser.rs` that:

- convert `f32` coordinates into `DragPoint`,
- update `visible_item_layouts` using visible indices mapped through `path_at_visible_index`,
- set `window` overlay properties in `apply_view`,
- keep ordinary empty-space click clearing available only when no drag threshold was crossed.

- [ ] **Step 4: Run targeted tests and build verification**

Run:

```bash
cargo test control_drag_keeps_unhit_baseline_items_selected -- --exact
cargo test browser_state_finish_drag_promotes_selection_focus_and_anchor -- --exact
cargo test browser_state_plain_workspace_click_without_drag_clears_selection -- --exact
cargo test
cargo build
```

Expected:
- targeted tests PASS,
- full `cargo test` PASS,
- `cargo build` PASS with the new Slint/Rust callback surface aligned.

- [ ] **Step 5: Commit**

If the project is under git, run:

```bash
git add src/browser.rs src/main.rs ui/app-window.slint
git commit -m "$(cat <<'EOF'
feat: add rectangle selection to file list

Wire Slint pointer input and overlay rendering into the Rust selection model so desktop-style rectangle selection works without moving business logic into the UI layer.
EOF
)"
```

If git is still unavailable, skip this step and continue.

### Task 4: Run final verification and record headless limits

**Files:**
- Modify: `README.md:104-110` (only if brief verification notes need updating)
- Modify: `开发交接说明.md:263-268` (only if brief verification notes need updating)
- Modify: `docs/superpowers/plans/2026-03-21-rectangle-selection.md`

- [ ] **Step 1: Add a verification checklist to the plan while the expected outcomes are fresh**

Append this checklist to the bottom of the plan file before running commands:

```markdown
## Verification Checklist

- [ ] `cargo test`
- [ ] `cargo build`
- [ ] `xvfb-run -a ./target/debug/files-rusted`
- [ ] Confirm plain empty-space click still clears selection
- [ ] Confirm item single-click/double-click semantics still work
- [ ] Confirm `Ctrl` drag keeps unhit baseline selections
- [ ] Confirm no-hit drag clears selection
- [ ] Note any headless-only limits that still need desktop validation
```

- [ ] **Step 2: Run verification commands**

Run from the project root:

```bash
cargo test
cargo build
xvfb-run -a ./target/debug/files-rusted
```

Expected:
- `cargo test` PASS
- `cargo build` PASS
- app starts under `xvfb-run` and remains running until externally stopped; any xdg color-scheme warning is non-blocking

- [ ] **Step 3: Record outcomes**

If the commands pass, keep the new verification checklist in the plan checked off during execution. If a command fails, write down whether the failure is:

- compile-time,
- runtime,
- or a headless-environment limitation.

Only update `README.md` or `开发交接说明.md` if the verification notes need a short factual refresh; do not expand scope into broader docs editing.

- [ ] **Step 4: Re-run the app smoke test after any fixes**

Run:

```bash
cargo build
xvfb-run -a ./target/debug/files-rusted
```

Expected: same successful startup result after the last code change.

- [ ] **Step 5: Commit**

If the project is under git and any doc/plan updates were made, run:

```bash
git add README.md "开发交接说明.md" docs/superpowers/plans/2026-03-21-rectangle-selection.md
git commit -m "$(cat <<'EOF'
chore: record rectangle selection verification

Capture the verification results and any headless-environment limits after wiring rectangle selection into the current MVP.
EOF
)"
```

If git is still unavailable, skip this step.
