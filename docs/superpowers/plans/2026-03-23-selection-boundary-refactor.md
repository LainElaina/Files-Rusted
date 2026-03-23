# Selection Boundary Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Files Rusted’s selection and rectangle-selection logic into focused submodules without changing the existing selection, keyboard, and drag semantics.

**Architecture:** Keep `BrowserState` in `src/browser.rs` as the public coordinator while extracting pure drag-selection geometry/session logic into `src/browser/drag_selection.rs` and core selection state mutations into `src/browser/selection.rs`. Preserve the current `main.rs` and Slint wiring surface as much as possible so behavior stays stable while internal responsibilities become clearer.

**Tech Stack:** Rust 2021, Slint 1.15.1, cargo test, cargo build, xvfb-run

---

## Preconditions

- Spec: `docs/superpowers/specs/2026-03-23-selection-boundary-design.md`
- Current repository: `/app/Files Rusted`
- Current branch: `main`
- Existing tests already cover current selection and drag-selection semantics in `src/browser.rs`; this refactor must preserve them.
- Keep scope limited to selection / drag-selection boundaries. Do not expand into navigation or file-operations refactors.

## File Map

- `src/browser.rs`
  - Remains the public browser coordinator and home of `BrowserState`.
  - Declares submodules and delegates selection-related work into them.
  - Keeps directory loading, navigation, file operations, and UI synchronization.
- `src/browser/selection.rs`
  - New focused module for selection state and selection mutation helpers.
  - Owns selected paths, primary/focus path, anchor path, and shared selection operations.
- `src/browser/drag_selection.rs`
  - New focused module for drag-selection geometry/session logic.
  - Owns drag point/rect/layout/session/result types and pure hit-testing / baseline-merge behavior.
- `src/main.rs`
  - Should need no behavior change; only adjust module paths/imports if compiler requires it.
- `ui/app-window.slint`
  - Should remain behaviorally unchanged for this refactor.

## Task 1: Extract drag-selection logic into `src/browser/drag_selection.rs`

**Files:**
- Create: `src/browser/drag_selection.rs`
- Modify: `src/browser.rs:1-260`
- Test: `src/browser.rs` (`#[cfg(test)]` tests moved or updated minimally)

- [ ] **Step 1: Write the failing test**

Add one focused regression test that proves the extracted drag-selection module must still expose the same behavior after the move:

```rust
#[test]
fn drag_selection_module_keeps_control_toggle_behavior() {
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
}
```

Put this alongside the current selection tests so the move is guarded by a targeted behavior assertion.

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::drag_selection_module_keeps_control_toggle_behavior -- --exact
```

Expected: FAIL once you start moving the drag-selection types out of `src/browser.rs` and before all imports/exports are correctly rewired.

- [ ] **Step 3: Write the minimal implementation**

Create `src/browser/drag_selection.rs` and move these focused types and helpers into it:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisibleItemLayout {
    pub path: PathBuf,
    pub rect: DragRect,
}

#[derive(Clone, Debug)]
pub struct DragSelectionSnapshot {
    pub selected: Vec<PathBuf>,
    pub primary: Option<PathBuf>,
    pub anchor: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DragSelectionResult {
    pub selected: Vec<PathBuf>,
    pub primary: Option<PathBuf>,
    pub anchor: Option<PathBuf>,
    pub rect: Option<DragRect>,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct DragSelectionSession {
    start: DragPoint,
    control: bool,
    baseline: DragSelectionSnapshot,
}
```

Also move the pure helpers that belong with them:

- `DragPoint::new`
- `DragRect::new`
- `DragRect::from_points`
- `DragRect::intersects`
- `DragSelectionSession::begin`
- `DragSelectionSession::selection_for`
- `drag_distance`
- `toggle_drag_selection`

In `src/browser.rs`, declare and import the module with exact code like:

```rust
mod browser {
    pub mod drag_selection;
    pub mod selection;
}
```

If the file remains `src/browser.rs`, use top-level module declarations instead:

```rust
#[path = "browser/drag_selection.rs"]
mod drag_selection;
#[path = "browser/selection.rs"]
mod selection;
```

Then import the moved items into `src/browser.rs` with `use crate::browser::drag_selection::...` or the correct sibling-module path required by the final layout.

Keep the tests in `src/browser.rs` for now if that is the smallest migration.

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::drag_selection_module_keeps_control_toggle_behavior -- --exact
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::drag_selection_replaces_selection_without_modifiers -- --exact
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::control_drag_toggles_against_baseline_selection -- --exact
```

Expected: PASS for all three tests.

- [ ] **Step 5: Commit**

Run:

```bash
git -C "/app/Files Rusted" add src/browser.rs src/browser/drag_selection.rs
git -C "/app/Files Rusted" commit -m "$(cat <<'EOF'
refactor: extract drag selection module

Move drag-selection geometry and session logic into a focused browser submodule so rectangle selection can evolve without further bloating BrowserState.
EOF
)"
```

## Task 2: Extract core selection state into `src/browser/selection.rs`

**Files:**
- Create: `src/browser/selection.rs`
- Modify: `src/browser.rs:12-1117`
- Test: `src/browser.rs` (`#[cfg(test)]` tests updated minimally)

- [ ] **Step 1: Write the failing test**

Add a regression test that protects the state-level selection semantics across the extraction:

```rust
#[test]
fn selection_state_keeps_ctrl_workspace_click_behavior() {
    let (state, _) = BrowserState::new(PathBuf::from("/workspace"));

    state.set_explicit_selection(
        vec![path("a.txt")],
        Some(path("a.txt")),
        Some(path("a.txt")),
    );
    state.begin_drag_selection(DragPoint::new(10.0, 10.0), true);
    state.finish_drag_selection();

    assert_eq!(*state.selected_paths.borrow(), vec![path("a.txt")]);
    assert_eq!(*state.primary_selected_path.borrow(), Some(path("a.txt")));
    assert_eq!(*state.selection_anchor_path.borrow(), Some(path("a.txt")));
}
```

This test should already pass before the extraction; it guards against wiring regressions during the move.

- [ ] **Step 2: Run the test to establish baseline**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::selection_state_keeps_ctrl_workspace_click_behavior -- --exact
```

Expected: PASS before the move. If it fails, stop and fix the regression before extracting anything.

- [ ] **Step 3: Write the minimal implementation**

Create `src/browser/selection.rs` with a focused state container, for example:

```rust
#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    pub primary_selected_path: Option<PathBuf>,
    pub selected_paths: Vec<PathBuf>,
    pub selection_anchor_path: Option<PathBuf>,
}
```

Move these helpers into the selection module (or convert them into `impl SelectionState` methods where it improves cohesion without widening scope):

- `select_range_to`
- `toggle_selection`
- `set_single_selection`
- `set_explicit_selection`
- `reconcile_selection`
- `ensure_selection_anchor`
- `clear_selection`
- `selected_items_for_operation`
- `set_focus_only`
- `normalize_operation_paths`

In `BrowserState`, replace the separate selection fields with a single focused selection-state field if it can be done cleanly, e.g.:

```rust
selection_state: RefCell<SelectionState>,
```

If that causes too much churn, an acceptable minimum is to move the logic first while temporarily keeping the fields in `BrowserState`. Prefer the cleaner `SelectionState` field if it does not force unrelated rewrites.

Update all internal callers in `src/browser.rs` to delegate through the new selection module.

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::selection_state_keeps_ctrl_workspace_click_behavior -- --exact
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::browser_state_finish_drag_promotes_selection_focus_and_anchor -- --exact
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::browser_state_plain_workspace_click_without_drag_clears_selection -- --exact
```

Expected: PASS for all targeted selection-state tests.

- [ ] **Step 5: Commit**

Run:

```bash
git -C "/app/Files Rusted" add src/browser.rs src/browser/selection.rs
git -C "/app/Files Rusted" commit -m "$(cat <<'EOF'
refactor: extract browser selection state

Isolate shared selection state and mutation logic so click, keyboard, and drag-selection behavior all flow through a smaller and clearer boundary.
EOF
)"
```

## Task 3: Reconcile module wiring and keep the public browser surface stable

**Files:**
- Modify: `src/browser.rs:1-1648`
- Modify: `src/main.rs:1-377` (only if imports/module paths require it)
- Test: `src/browser.rs`

- [ ] **Step 1: Write the failing integration check**

Add a focused end-to-end state regression that exercises the current `BrowserState` public surface after both extractions:

```rust
#[test]
fn browser_state_public_selection_flow_still_works_after_refactor() {
    let (state, _) = BrowserState::new(PathBuf::from("/workspace"));
    let layouts = vec![
        layout("a.txt", 0.0, 0.0, 300.0, 84.0),
        layout("b.txt", 0.0, 92.0, 300.0, 84.0),
        layout("c.txt", 0.0, 184.0, 300.0, 84.0),
    ];

    state.replace_visible_item_layouts(layouts);
    state.begin_drag_selection(DragPoint::new(0.0, 0.0), false);
    state.update_drag_selection(DragPoint::new(280.0, 150.0));
    state.finish_drag_selection();

    assert_eq!(*state.selected_paths.borrow(), vec![path("a.txt"), path("b.txt")]);
}
```

This makes sure the extraction did not accidentally break the coordinator API that `main.rs` and the UI depend on.

- [ ] **Step 2: Run the test to verify current behavior**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test browser::tests::browser_state_public_selection_flow_still_works_after_refactor -- --exact
```

Expected: PASS once the delegated wiring is correct.

- [ ] **Step 3: Write the minimal implementation**

Clean up `src/browser.rs` so it clearly acts as coordinator only:

- group imports by module responsibility,
- re-export or import the selection/drag types cleanly,
- keep current public `BrowserState` methods stable where possible,
- remove now-duplicated helper logic from `src/browser.rs`,
- ensure `main.rs` continues compiling with either no change or the smallest import adjustment required.

Do **not** change `ui/app-window.slint` unless the compiler forces a path-level fix.

- [ ] **Step 4: Run tests and build to verify integration**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo build
```

Expected:
- all tests PASS,
- build PASS,
- no behavior regressions implied by the current test suite.

- [ ] **Step 5: Commit**

Run:

```bash
git -C "/app/Files Rusted" add src/browser.rs src/browser/selection.rs src/browser/drag_selection.rs src/main.rs
git -C "/app/Files Rusted" commit -m "$(cat <<'EOF'
refactor: keep browser state as selection coordinator

Finish the selection-boundary refactor by delegating drag and selection internals into focused modules while preserving the current public browser surface.
EOF
)"
```

## Task 4: Final verification and smoke test

**Files:**
- Modify: `docs/superpowers/plans/2026-03-23-selection-boundary-refactor.md`
- Modify: `README.md` or `开发交接说明.md` only if a short factual verification note is necessary

- [ ] **Step 1: Append a verification checklist to the plan**

Append this exact checklist to the bottom of the plan file before running final commands:

```markdown
## Verification Checklist

- [ ] `cargo test`
- [ ] `cargo build`
- [ ] `xvfb-run -a ./target/debug/files-rusted`
- [ ] Confirm rectangle selection tests still pass
- [ ] Confirm plain empty-space click still clears selection
- [ ] Confirm `Ctrl +` empty-space click still keeps selection
- [ ] Confirm no behavior changes were required in the Slint UI layer
```

- [ ] **Step 2: Run the full verification commands**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo test
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo build
cd "/app/Files Rusted" && timeout 15s xvfb-run -a ./target/debug/files-rusted
```

Expected:
- `cargo test` PASS
- `cargo build` PASS
- app starts under `xvfb-run` and remains alive until `timeout` stops it
- the xdg color-scheme warning may appear and is non-blocking

- [ ] **Step 3: Record factual outcomes only if needed**

If verification passes, check off the plan checklist during execution. Only update `README.md` or `开发交接说明.md` if there is a short factual note worth refreshing; do not expand scope into broader documentation work.

- [ ] **Step 4: Re-run the smoke test after any late fixes**

Run:

```bash
cd "/app/Files Rusted" && . "$HOME/.cargo/env" && cargo build
cd "/app/Files Rusted" && timeout 15s xvfb-run -a ./target/debug/files-rusted
```

Expected: same successful startup result after the final code change.

- [ ] **Step 5: Commit**

Run:

```bash
git -C "/app/Files Rusted" add docs/superpowers/plans/2026-03-23-selection-boundary-refactor.md README.md "开发交接说明.md"
git -C "/app/Files Rusted" commit -m "$(cat <<'EOF'
chore: record selection refactor verification

Capture the verification status for the selection-boundary refactor after tests, build, and headless startup all pass.
EOF
)"
```

If no docs changed, stage only the plan file.
