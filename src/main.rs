mod browser;

use browser::BrowserState;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{env, path::PathBuf, rc::Rc};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    let start_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let (state, sidebar_entries) = BrowserState::new(start_dir);
    let state = Rc::new(state);

    let sidebar_model = Rc::new(VecModel::from(sidebar_entries));
    let file_model = Rc::new(VecModel::from(Vec::<FileEntry>::new()));
    let sort_options_model = Rc::new(VecModel::from(BrowserState::sort_options()));
    let breadcrumb_model = Rc::new(VecModel::from(Vec::<BreadcrumbEntry>::new()));

    window.set_sidebar_items(ModelRc::from(sidebar_model.clone()));
    window.set_file_items(ModelRc::from(file_model.clone()));
    window.set_sort_options(ModelRc::from(sort_options_model.clone()));
    window.set_breadcrumb_items(ModelRc::from(breadcrumb_model.clone()));
    window.set_current_sort_index(state.current_sort_index());

    state.refresh(&window, file_model.as_ref());

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_create_file(move || {
            if let Some(window) = window_weak.upgrade() {
                state.create_file(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_request_copy(move || {
            if let Some(window) = window_weak.upgrade() {
                state.request_copy_selected(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_request_cut(move || {
            if let Some(window) = window_weak.upgrade() {
                state.request_cut_selected(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_paste_into_current_directory(move || {
            if let Some(window) = window_weak.upgrade() {
                state.paste_into_current_dir(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_open_selected(move || {
            if let Some(window) = window_weak.upgrade() {
                state.open_selected(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_create_folder(move || {
            if let Some(window) = window_weak.upgrade() {
                state.create_folder(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_request_rename(move || {
            if let Some(window) = window_weak.upgrade() {
                state.request_rename_selected(&window, file_model.as_ref());
            }
        });
    }

    {
        let state = state.clone();
        window.on_rename_draft_updated(move |value| {
            state.set_rename_draft(value.to_string());
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_commit_rename(move || {
            if let Some(window) = window_weak.upgrade() {
                state.commit_rename(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_cancel_rename(move || {
            if let Some(window) = window_weak.upgrade() {
                state.cancel_rename(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_delete_selected(move || {
            if let Some(window) = window_weak.upgrade() {
                state.delete_selected(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_navigate_back(move || {
            if let Some(window) = window_weak.upgrade() {
                state.navigate_back(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_navigate_forward(move || {
            if let Some(window) = window_weak.upgrade() {
                state.navigate_forward(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_navigate_home(move || {
            if let Some(window) = window_weak.upgrade() {
                state.navigate_home(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_navigate_up(move || {
            if let Some(window) = window_weak.upgrade() {
                state.navigate_up(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_refresh_directory(move || {
            if let Some(window) = window_weak.upgrade() {
                state.refresh(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_activate_sidebar(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.activate_sidebar(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_activate_breadcrumb(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.activate_breadcrumb(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_request_copy_item(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.request_copy_item(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_request_cut_item(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.request_cut_item(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_request_rename_item(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.request_rename_item(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_delete_item(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.delete_item(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_begin_drag_selection(move |x, y, control| {
            if let Some(window) = window_weak.upgrade() {
                state.begin_drag_selection_from_ui(x, y, control, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_update_drag_selection(move |x, y| {
            if let Some(window) = window_weak.upgrade() {
                state.update_drag_selection_from_ui(x, y, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_finish_drag_selection(move || {
            if let Some(window) = window_weak.upgrade() {
                state.finish_drag_selection_from_ui(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_clear_visible_item_layouts(move || {
            if let Some(window) = window_weak.upgrade() {
                state.clear_visible_item_layouts(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_register_visible_item_layout(move |index, x, y, width, height| {
            if let Some(window) = window_weak.upgrade() {
                state.register_visible_item_layout(index, x, y, width, height, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_request_workspace_clear_selection(move || {
            if let Some(window) = window_weak.upgrade() {
                if state.has_active_drag_selection() {
                    state.finish_drag_selection_from_ui(&window, file_model.as_ref());
                } else {
                    state.clear_selection_command(&window, file_model.as_ref());
                }
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_activate_file(move |index, control, shift| {
            if let Some(window) = window_weak.upgrade() {
                state.activate_file(index, control, shift, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_open_item(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.open_item(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_select_next(move |extend, control| {
            if let Some(window) = window_weak.upgrade() {
                state.move_focus_next(extend, control, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_select_previous(move |extend, control| {
            if let Some(window) = window_weak.upgrade() {
                state.move_focus_previous(extend, control, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_select_boundary(move |to_end, extend, control| {
            if let Some(window) = window_weak.upgrade() {
                state.move_focus_to_boundary(to_end, extend, control, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_toggle_focused_selection(move |extend, control| {
            if let Some(window) = window_weak.upgrade() {
                state.toggle_focused_selection(extend, control, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_select_all(move || {
            if let Some(window) = window_weak.upgrade() {
                state.select_all(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_clear_selection(move || {
            if let Some(window) = window_weak.upgrade() {
                state.clear_selection_command(&window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_sort_mode_selected(move |index| {
            if let Some(window) = window_weak.upgrade() {
                state.set_sort_mode(index, &window, file_model.as_ref());
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let state = state.clone();
        let file_model = file_model.clone();
        window.on_filter_updated(move |query| {
            if let Some(window) = window_weak.upgrade() {
                state.set_filter_query(query.to_string(), &window, file_model.as_ref());
            }
        });
    }

    window.run()
}
