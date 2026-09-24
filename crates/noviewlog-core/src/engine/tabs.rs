//! Filter-tab lifecycle: add, close, restore, switch, rename, reorder tabs.

use super::*;

impl Engine {
    pub(crate) fn add_tab(&mut self) {
        let tab_count = self.active_terminal().views.len();
        let name = format!("Tab {}", tab_count + 1);
        let is_file = self.active_terminal().is_file_session();
        let terminal = self.active_terminal_mut();
        // Filter tabs start empty (full stream); user adds include/exclude rules.
        let mut view = LogView::from_runtime(&name, Vec::new());
        if is_file {
            view.auto_follow = false;
        }
        terminal.views.push(view);
        terminal.active_view = terminal.views.len() - 1;
        terminal.scroll_offset_y = 0.0;
        terminal.scroll_x = 0.0;
        terminal.selection = None;
        self.mark_viewport_dirty();
        // Flush tab strip chrome on the next tick (do not wait for the 250ms stats throttle).
        self.last_stats_at = None;
        self.sync_active_project_from_terminals();
        let _ = self.rebuild_if_needed();
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        let terminal = self.active_terminal_mut();
        // Tab 0 is the Terminal tab — never close it.
        if index == 0 || terminal.views.len() <= 1 || index >= terminal.views.len() {
            return;
        }
        let tab = terminal.views[index].to_tab_config();
        terminal.views.remove(index);
        terminal.closed_tabs.push_back(tab);
        while terminal.closed_tabs.len() > MAX_CLOSED_TABS {
            terminal.closed_tabs.pop_front();
        }
        if terminal.active_view >= terminal.views.len() {
            terminal.active_view = terminal.views.len() - 1;
        } else if index < terminal.active_view {
            terminal.active_view -= 1;
        }
        terminal.scroll_offset_y = 0.0;
        terminal.scroll_x = 0.0;
        terminal.selection = None;
        self.mark_viewport_dirty();
        self.last_stats_at = None;
        self.sync_active_project_from_terminals();
        let _ = self.rebuild_if_needed();
    }

    pub(crate) fn restore_tab(&mut self) {
        let terminal = self.active_terminal_mut();
        let Some(tab) = terminal.closed_tabs.pop_back() else {
            return;
        };
        terminal.views.push(LogView::from_tab_config(tab));
        terminal.active_view = terminal.views.len() - 1;
        terminal.scroll_offset_y = 0.0;
        terminal.scroll_x = 0.0;
        terminal.selection = None;
        self.mark_viewport_dirty();
        self.last_stats_at = None;
        self.sync_active_project_from_terminals();
        let _ = self.rebuild_if_needed();
    }

    pub(crate) fn switch_tab(&mut self, index: usize) {
        let terminal = self.active_terminal_mut();
        if index < terminal.views.len() && index != terminal.active_view {
            let is_file = terminal.is_file_session();
            terminal.active_view = index;
            terminal.scroll_offset_y = 0.0;
            terminal.scroll_x = 0.0;
            terminal.selection = None;
            // Live filter tabs scan the ≤30k Record ring on select (not a
            // FILES match index). Overlay snapshot is applied in rebuild_if_needed.
            if index != 0 && !is_file {
                terminal.views[index].mark_flat_lines_dirty();
            }
            self.mark_viewport_dirty();
            self.last_stats_at = None;
            self.sync_active_project_from_terminals();
            let _ = self.rebuild_if_needed();
        }
    }

    pub(crate) fn rename_tab(&mut self, index: usize, name: &str) {
        let name = name.trim();
        let terminal = self.active_terminal_mut();
        // Tab 0 is the Terminal tab — never rename it (UI also blocks; this is defense in depth).
        if name.is_empty() || index >= terminal.views.len() || index == 0 {
            return;
        }
        terminal.views[index].name = name.to_string();
        self.sync_active_project_from_terminals();
    }

    /// Reorder filter tabs. The Terminal tab stays at index 0 (`from`/`to` of 0 are no-ops).
    pub(crate) fn tab_move(&mut self, from_index: usize, to_index: usize) {
        let terminal = self.active_terminal_mut();
        let len = terminal.views.len();
        if len < 2 || from_index == 0 || to_index == 0 {
            return;
        }
        if from_index >= len || to_index >= len || from_index == to_index {
            return;
        }
        let item = terminal.views.remove(from_index);
        terminal.views.insert(to_index, item);
        let active = terminal.active_view;
        terminal.active_view = if active == from_index {
            to_index
        } else if from_index < active && to_index >= active {
            active - 1
        } else if from_index > active && to_index <= active {
            active + 1
        } else {
            active
        };
        // Tab strip chrome; viewport content may change if active moved.
        self.mark_viewport_dirty();
        self.last_stats_at = None;
        self.sync_active_project_from_terminals();
    }
}
