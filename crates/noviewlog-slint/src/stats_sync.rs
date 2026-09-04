//! Push typed engine [`StatsSnapshot`] into Slint models / chrome properties.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use noviewlog_core::core::types::{clamp_max_scrollback_lines, FilterType};
use noviewlog_core::{StatsSnapshot, StatsTerminal, TERMINAL_TAB_NAME};
use slint::{Model, SharedString, Timer, VecModel};

use crate::ui::{AppWindow, FilterInfo, ProjectInfo, TabInfo, TerminalInfo};

/// Pending debounced find `search_set` payload: query, regex, case, whole-word.
pub(crate) type FindPending = Option<(String, bool, bool, bool)>;

/// Apply one typed stats snapshot to all Slint chrome / models.
///
/// Returns `true` when tab, terminal, or filter models changed (caller may force paint).
pub(crate) fn apply_stats(
    stats: &StatsSnapshot,
    tabs: &Rc<VecModel<TabInfo>>,
    terminals: &Rc<VecModel<TerminalInfo>>,
    files: &Rc<VecModel<TerminalInfo>>,
    projects: &Rc<VecModel<ProjectInfo>>,
    filters: &Rc<VecModel<FilterInfo>>,
    ui: &AppWindow,
    terminal_tab_active: &Rc<Cell<bool>>,
    syncing_scroll: &Rc<Cell<bool>>,
    has_selection: &Rc<Cell<bool>>,
    pty_running: &Rc<Cell<bool>>,
    viewport_font_size: &Rc<Cell<f32>>,
    syncing_follow: &Rc<Cell<bool>>,
    find_resync: &Rc<Cell<bool>>,
    find_stats_tab: &Rc<Cell<i32>>,
    find_pending: &Rc<RefCell<FindPending>>,
    find_debounce: &Rc<Timer>,
) -> bool {
    let tabs_changed = apply_stats_to_tabs(stats, tabs, ui, terminal_tab_active);
    let terms_changed = apply_stats_to_terminals(stats, terminals, files, projects, ui);
    let filters_changed = apply_stats_to_filters(stats, filters, ui);
    apply_stats_to_find(
        stats,
        ui,
        find_resync,
        find_stats_tab,
        find_pending,
        find_debounce,
    );
    apply_stats_to_scroll(stats, ui, syncing_scroll);
    apply_stats_to_selection(stats, has_selection, ui);
    apply_stats_to_running(stats, pty_running);
    apply_stats_to_view_chrome(stats, ui, viewport_font_size, syncing_follow);
    tabs_changed || terms_changed || filters_changed
}

fn apply_stats_to_tabs(
    stats: &StatsSnapshot,
    tabs: &Rc<VecModel<TabInfo>>,
    ui: &AppWindow,
    terminal_tab_active: &Rc<Cell<bool>>,
) -> bool {
    let active = stats.active_tab as i32;
    let can_restore = stats.can_restore_closed_tab;
    let is_terminal_tab = stats.is_terminal_tab;

    let mut changed = false;
    if ui.get_active_tab_index() != active {
        ui.set_active_tab_index(active);
        changed = true;
    }
    if ui.get_can_restore_tab() != can_restore {
        ui.set_can_restore_tab(can_restore);
        changed = true;
    }
    terminal_tab_active.set(is_terminal_tab);

    let next: Vec<TabInfo> = stats
        .tabs
        .iter()
        .map(|tab| {
            let index = tab.index as i32;
            let name = if tab.name.is_empty() {
                if index == 0 {
                    TERMINAL_TAB_NAME
                } else {
                    "Tab"
                }
            } else {
                tab.name.as_str()
            };
            TabInfo {
                index,
                name: SharedString::from(name),
                is_terminal_tab: tab.is_terminal_tab,
            }
        })
        .collect();

    if tabs_model_differs(tabs, &next) {
        tabs.set_vec(next);
        changed = true;
    }

    changed
}

fn apply_stats_to_scroll(stats: &StatsSnapshot, ui: &AppWindow, syncing: &Rc<Cell<bool>>) {
    let scroll_y = stats.scroll_y;
    let max_scroll_y = stats.max_scroll_y;
    let scroll_x = stats.scroll_x;
    let max_scroll_x = stats.max_scroll_x;
    let wrap_lines = stats.wrap_lines;
    let show_h = !wrap_lines && max_scroll_x > 0.5;

    syncing.set(true);
    let max_y = max_scroll_y.max(0.0);
    let max_x = max_scroll_x.max(0.0);
    let next_y = scroll_y.clamp(0.0, max_y);
    let next_x = scroll_x.clamp(0.0, max_x);
    // Always refresh extents (thumb size); skip scroll position if unchanged to avoid churn.
    ui.set_max_scroll_y(max_y);
    ui.set_max_scroll_x(max_x);
    if (ui.get_scroll_y() - next_y).abs() > 0.5 {
        ui.set_scroll_y(next_y);
    }
    if (ui.get_scroll_x() - next_x).abs() > 0.5 {
        ui.set_scroll_x(next_x);
    }
    if ui.get_show_hscroll() != show_h {
        ui.set_show_hscroll(show_h);
    }
    syncing.set(false);
}

fn apply_stats_to_terminals(
    stats: &StatsSnapshot,
    terminals: &Rc<VecModel<TerminalInfo>>,
    files: &Rc<VecModel<TerminalInfo>>,
    projects: &Rc<VecModel<ProjectInfo>>,
    ui: &AppWindow,
) -> bool {
    let active = stats.active_terminal as i32;

    let mut changed = false;
    if ui.get_active_terminal_index() != active {
        ui.set_active_terminal_index(active);
        changed = true;
    }
    if ui.get_is_file_session() != stats.is_file_session {
        ui.set_is_file_session(stats.is_file_session);
        changed = true;
    }
    if ui.get_terminals_section_expanded() != stats.terminals_section_expanded {
        ui.set_terminals_section_expanded(stats.terminals_section_expanded);
        changed = true;
    }
    if ui.get_files_section_expanded() != stats.files_section_expanded {
        ui.set_files_section_expanded(stats.files_section_expanded);
        changed = true;
    }

    let active_project_id = stats
        .active_project_id
        .as_deref()
        .unwrap_or("");
    if ui.get_active_project_id().as_str() != active_project_id {
        ui.set_active_project_id(SharedString::from(active_project_id));
        changed = true;
    }
    let active_project_name = stats
        .active_project_id
        .as_ref()
        .and_then(|id| stats.projects.iter().find(|p| &p.id == id))
        .map(|p| p.name.as_str())
        .unwrap_or("");
    if ui.get_active_project_name().as_str() != active_project_name {
        ui.set_active_project_name(SharedString::from(active_project_name));
        changed = true;
    }

    let next_terms: Vec<TerminalInfo> = stats
        .terminals
        .iter()
        .map(|term| stats_terminal_to_info(term))
        .collect();
    if terminals_model_differs(terminals, &next_terms) {
        terminals.set_vec(next_terms);
        changed = true;
    }

    let next_files: Vec<TerminalInfo> = stats
        .files
        .iter()
        .map(|term| stats_terminal_to_info(term))
        .collect();
    if terminals_model_differs(files, &next_files) {
        files.set_vec(next_files);
        changed = true;
    }

    let next_projects: Vec<ProjectInfo> = stats
        .projects
        .iter()
        .map(|p| ProjectInfo {
            index: p.index as i32,
            id: SharedString::from(p.id.as_str()),
            name: SharedString::from(p.name.as_str()),
            program_count: p.program_count as i32,
            active: stats
                .active_project_id
                .as_ref()
                .is_some_and(|id| id == &p.id),
        })
        .collect();
    if projects_model_differs(projects, &next_projects) {
        projects.set_vec(next_projects);
        changed = true;
    }

    changed
}

fn stats_terminal_to_info(term: &StatsTerminal) -> TerminalInfo {
    let label = if term.label.is_empty() {
        "."
    } else {
        term.label.as_str()
    };
    TerminalInfo {
        index: term.index as i32,
        id: SharedString::from(term.id.as_str()),
        label: SharedString::from(label),
        cwd: SharedString::from(term.cwd.as_str()),
        running: term.running,
        has_launch: term.has_launch,
        launch_command: SharedString::from(term.launch_command.as_str()),
        launch_args: SharedString::from(term.launch_args.as_str()),
        launch_cwd: SharedString::from(term.launch_cwd.as_str()),
        launch_wsl: term.launch_wsl,
        launch_wsl_distro: SharedString::from(term.launch_wsl_distro.as_str()),
    }
}

fn tabs_model_differs(model: &VecModel<TabInfo>, next: &[TabInfo]) -> bool {
    if model.row_count() != next.len() {
        return true;
    }
    for (i, tab) in next.iter().enumerate() {
        let Some(cur) = model.row_data(i) else {
            return true;
        };
        if cur.index != tab.index || cur.is_terminal_tab != tab.is_terminal_tab || cur.name != tab.name {
            return true;
        }
    }
    false
}

fn terminals_model_differs(model: &VecModel<TerminalInfo>, next: &[TerminalInfo]) -> bool {
    if model.row_count() != next.len() {
        return true;
    }
    for (i, term) in next.iter().enumerate() {
        let Some(cur) = model.row_data(i) else {
            return true;
        };
        if cur.index != term.index
            || cur.running != term.running
            || cur.id != term.id
            || cur.label != term.label
            || cur.cwd != term.cwd
            || cur.has_launch != term.has_launch
            || cur.launch_command != term.launch_command
            || cur.launch_args != term.launch_args
            || cur.launch_cwd != term.launch_cwd
            || cur.launch_wsl != term.launch_wsl
            || cur.launch_wsl_distro != term.launch_wsl_distro
        {
            return true;
        }
    }
    false
}

fn projects_model_differs(model: &VecModel<ProjectInfo>, next: &[ProjectInfo]) -> bool {
    if model.row_count() != next.len() {
        return true;
    }
    for (i, proj) in next.iter().enumerate() {
        let Some(cur) = model.row_data(i) else {
            return true;
        };
        if cur.index != proj.index
            || cur.id != proj.id
            || cur.name != proj.name
            || cur.program_count != proj.program_count
            || cur.active != proj.active
        {
            return true;
        }
    }
    false
}

fn apply_stats_to_filters(
    stats: &StatsSnapshot,
    filters: &Rc<VecModel<FilterInfo>>,
    ui: &AppWindow,
) -> bool {
    let active = stats.active_tab as i32;
    let is_terminal_tab = stats.is_terminal_tab;
    let editable = !is_terminal_tab && active != 0;
    let mut changed = false;
    if ui.get_filters_editable() != editable {
        ui.set_filters_editable(editable);
        changed = true;
    }

    let next: Vec<FilterInfo> = stats
        .filters
        .iter()
        .map(|f| {
            let filter_type = match f.filter_type {
                FilterType::Include => "include",
                FilterType::Exclude => "exclude",
            };
            FilterInfo {
                id: SharedString::from(f.id.as_str()),
                filter_type: SharedString::from(filter_type),
                pattern: SharedString::from(f.pattern.as_str()),
                enabled: f.enabled,
                use_regex: f.use_regex,
            }
        })
        .collect();

    if filters.row_count() == next.len()
        && (0..next.len()).all(|i| {
            filters
                .row_data(i)
                .is_some_and(|cur| cur.id == next[i].id)
        })
    {
        for (i, filt) in next.into_iter().enumerate() {
            let Some(cur) = filters.row_data(i) else {
                continue;
            };
            if cur.filter_type != filt.filter_type
                || cur.pattern != filt.pattern
                || cur.enabled != filt.enabled
                || cur.use_regex != filt.use_regex
            {
                filters.set_row_data(i, filt);
                changed = true;
            }
        }
        return changed;
    }

    if filters_model_differs(filters, &next) {
        filters.set_vec(next);
        changed = true;
    }

    changed
}

fn filters_model_differs(model: &VecModel<FilterInfo>, next: &[FilterInfo]) -> bool {
    if model.row_count() != next.len() {
        return true;
    }
    for (i, filt) in next.iter().enumerate() {
        let Some(cur) = model.row_data(i) else {
            return true;
        };
        if cur.id != filt.id
            || cur.filter_type != filt.filter_type
            || cur.pattern != filt.pattern
            || cur.enabled != filt.enabled
            || cur.use_regex != filt.use_regex
        {
            return true;
        }
    }
    false
}

fn apply_stats_to_find(
    stats: &StatsSnapshot,
    ui: &AppWindow,
    find_resync: &Rc<Cell<bool>>,
    find_stats_tab: &Rc<Cell<i32>>,
    find_pending: &Rc<RefCell<FindPending>>,
    find_debounce: &Rc<Timer>,
) {
    if !ui.get_find_open() {
        return;
    }

    let active = stats.active_tab as i32;
    let tab_changed = find_stats_tab.get() != active;
    find_stats_tab.set(active);
    // Tab switch: drop in-flight typing for the previous tab.
    if tab_changed {
        find_debounce.stop();
        find_pending.borrow_mut().take();
    }
    let open_resync = find_resync.replace(false);
    let has_pending = find_pending.borrow().is_some();
    // Never clobber the Find field while a debounced edit is in flight — that
    // race made the query snap back to the engine's (often empty) search when
    // focus left the TextInput and stats applied.
    let resync = tab_changed || (open_resync && !has_pending);

    let query = stats.search_query.as_str();
    let regex = stats.search_regex;
    let case_sensitive = stats.search_case_sensitive;
    let whole_word = stats.search_whole_word;
    let counter = stats.search_counter.as_str();
    let error = stats.search_error.as_deref().unwrap_or("");

    // Only push query/toggles on open or tab switch — never while the user types.
    if resync {
        let ui_query = ui.get_find_query();
        // If the user already typed into an empty engine search, keep the UI text.
        if tab_changed || query != ui_query.as_str() {
            if !(query.is_empty() && !ui_query.is_empty() && !tab_changed) {
                ui.set_find_query(SharedString::from(query));
            }
        }
        ui.set_find_regex(regex);
        ui.set_find_case_sensitive(case_sensitive);
        ui.set_find_whole_word(whole_word);
    }

    let ui_query = ui.get_find_query();
    let status_query = if resync && !query.is_empty() {
        query
    } else {
        ui_query.as_str()
    };
    // Status counter is engine-truth; only show it when it matches the UI query
    // (avoids "No results" flash for text not yet flushed via debounce).
    let counter_applies = !has_pending && ui_query.as_str() == query;
    let status = if !error.is_empty() && counter_applies {
        SharedString::default()
    } else if status_query.is_empty() {
        SharedString::default()
    } else if !counter_applies {
        SharedString::default()
    } else if counter == "0/0" {
        SharedString::from("No results")
    } else if counter.is_empty() {
        SharedString::default()
    } else if let Some((cur, total)) = counter.split_once('/') {
        SharedString::from(format!("{cur} of {total}"))
    } else {
        SharedString::from(counter)
    };
    ui.set_find_status(status);
    if counter_applies {
        ui.set_find_error(SharedString::from(error));
    } else if !has_pending {
        ui.set_find_error(SharedString::default());
    }
}

fn apply_stats_to_selection(stats: &StatsSnapshot, has_selection: &Rc<Cell<bool>>, ui: &AppWindow) {
    let selected = stats.has_selection;
    has_selection.set(selected);
    ui.set_can_copy(selected);
}

fn apply_stats_to_running(stats: &StatsSnapshot, pty_running: &Rc<Cell<bool>>) {
    pty_running.set(stats.running);
}

fn apply_stats_to_view_chrome(
    stats: &StatsSnapshot,
    ui: &AppWindow,
    viewport_font_size: &Rc<Cell<f32>>,
    syncing_follow: &Rc<Cell<bool>>,
) {
    viewport_font_size.set(stats.viewport_font_size);

    let line_pos = if stats.viewport_line_total > 0 && stats.viewport_line > 0 {
        SharedString::from(format!(
            "{} / {}",
            stats.viewport_line, stats.viewport_line_total
        ))
    } else {
        SharedString::default()
    };
    if ui.get_line_position_text() != line_pos {
        ui.set_line_position_text(line_pos);
    }

    let capped = clamp_max_scrollback_lines(stats.max_scrollback_lines) as i32;
    if ui.get_max_scrollback_lines() != capped {
        ui.set_max_scrollback_lines(capped);
    }

    let wrap_lines = stats.wrap_lines;
    if ui.get_wrap_lines() != wrap_lines {
        ui.set_wrap_lines(wrap_lines);
    }

    let severity = if stats.severity_filter.is_empty() {
        "all"
    } else {
        stats.severity_filter.as_str()
    };
    if ui.get_severity_mode().as_str() != severity {
        ui.set_severity_mode(SharedString::from(severity));
    }

    // Slint `auto-follow` / callback `set-follow` ↔ engine `SetFollow` / stats `auto_follow`.
    let auto_follow = stats.auto_follow;
    if ui.get_auto_follow() != auto_follow {
        // Guard: if property write ever re-enters on_set_follow, do not echo SetFollow.
        syncing_follow.set(true);
        ui.set_auto_follow(auto_follow);
        syncing_follow.set(false);
    }

    let tab_count = stats.tab_count as i32;
    let active = stats.active_tab as i32;
    let is_terminal_tab = stats.is_terminal_tab;
    let can_restore = stats.can_restore_closed_tab;
    let on_terminal_tab = is_terminal_tab || active == 0;
    let can_close = tab_count > 1 && !on_terminal_tab;
    if ui.get_can_close_tab() != can_close {
        ui.set_can_close_tab(can_close);
    }
    if ui.get_can_restore_tab() != can_restore {
        ui.set_can_restore_tab(can_restore);
    }
    // The Terminal tab must not be renamed — mirror Close enablement for Tab → Rename.
    let can_rename = !on_terminal_tab;
    if ui.get_can_rename_tab() != can_rename {
        ui.set_can_rename_tab(can_rename);
    }
    // Drop inline rename if the target tab vanished (e.g. closed elsewhere).
    let renaming = ui.get_renaming_tab_index();
    if renaming >= 0 && renaming >= tab_count {
        ui.set_renaming_tab_index(-1);
        if ui.get_renaming_terminal_id().is_empty() {
            ui.set_rename_draft(SharedString::default());
        }
    }

    // Drop terminal rename if the target id vanished (or never matched after stats).
    let renaming_tid = ui.get_renaming_terminal_id();
    if !renaming_tid.is_empty()
        && !stats
            .terminals
            .iter()
            .chain(stats.files.iter())
            .any(|t| t.id.as_str() == renaming_tid.as_str())
    {
        ui.set_renaming_terminal_id(SharedString::default());
        if ui.get_renaming_tab_index() < 0 {
            ui.set_rename_draft(SharedString::default());
        }
    }
}
