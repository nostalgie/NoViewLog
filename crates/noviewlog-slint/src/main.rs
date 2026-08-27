//! NoViewLog Slint prototype: viewport, tabs, terminals sidebar.

mod app_state;
mod engine_bridge;
mod input;
mod launch_args;
mod stats_sync;
mod ui;
mod window_chrome;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use noviewlog_core::core::types::{
    clamp_max_scrollback_lines, FilterType, DEFAULT_MAX_SCROLLBACK_LINES,
};
use noviewlog_core::{parse_engine_event, Command, Engine, EngineEvent, CARET_BLINK_PERIOD};
use slint::{
    ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer,
    TimerMode, VecModel,
};

// UI-thread tick body; PTY wake uses `invoke_from_event_loop` → this TLS (Send-safe).
thread_local! {
    static HOST_TICK: RefCell<Option<Box<dyn FnMut()>>> = const { RefCell::new(None) };
}

use crate::app_state::ClickTracker;
use crate::engine_bridge::{
    bump_fast_timer, find_terminal_index, set_occluded_timer, window_should_pause_paint, TICK_FAST,
    TICK_IDLE,
};
use crate::input::{
    clipboard_has_text, copy_selection_to_clipboard, handle_key_event, is_key_char, is_zoom_in_key,
    paste_clipboard_to_console,
};
use crate::stats_sync::apply_stats;
use crate::ui::*;

/// Debounce for find `search_set` (search bar cadence).
const FIND_DEBOUNCE: Duration = Duration::from_millis(150);
/// Debounce for FILTERS draft highlight preview.
const FILTER_DRAFT_DEBOUNCE: Duration = Duration::from_millis(150);
/// Placeholder fill matching `Theme.bg-window` (`#0d1117`) so the first `Image`
/// is opaque on a transparent winit swapchain.
const VIEWPORT_PLACEHOLDER_RGBA: [u8; 4] = [0x0d, 0x11, 0x17, 0xff];

fn seed_opaque_viewport(ui: &AppWindow) {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(8, 8);
    for px in buffer.make_mut_bytes().chunks_exact_mut(4) {
        px.copy_from_slice(&VIEWPORT_PLACEHOLDER_RGBA);
    }
    ui.set_viewport_image(Image::from_rgba8(buffer));
}

/// Sync Slint caret overlay from engine geometry (device px → logical).
/// Returns whether the overlay is shown.
fn sync_console_caret(ui: &AppWindow, eng: &Engine, width: u32, height: u32, scale: f32) -> bool {
    if !eng.console_caret_active() {
        ui.set_caret_visible(false);
        return false;
    }
    let Some((x, y, w, h)) = eng.console_caret_rect(width, height) else {
        ui.set_caret_visible(false);
        return false;
    };
    let scale = scale.max(0.5);
    ui.set_caret_x(x / scale);
    ui.set_caret_y(y / scale);
    ui.set_caret_w(w / scale);
    ui.set_caret_h(h / scale);
    ui.set_caret_visible(true);
    true
}

/// Focus Console viewport + engine flag + overlay (startup / tab switch).
fn arm_console_caret(ui: &AppWindow, eng: &mut Engine, logical: (f32, f32)) {
    ui.invoke_focus_viewport();
    let _ = eng.send_command(Command::SetViewportFocus { focused: true });
    eng.reset_caret_blink();
    ui.set_caret_blink_on(true);
    let scale = ui.window().scale_factor().max(0.5) as f32;
    let width = (logical.0 * scale).ceil().max(1.0) as u32;
    let height = (logical.1 * scale).ceil().max(1.0) as u32;
    let _ = sync_console_caret(ui, eng, width, height, scale);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Slint defaults to `with_transparent(true)` for FemtoVG WGPU. On Wayland that
    // yields a translucent swapchain: empty/alpha-0 regions show the launching
    // terminal until a full opaque paint. Force an opaque window surface.
    slint::BackendSelector::new()
        .with_winit_window_attributes_hook(|attrs| attrs.with_transparent(false))
        .select()?;

    let ui = AppWindow::new()?;

    let tabs_model = Rc::new(VecModel::<TabInfo>::from(vec![TabInfo {
        index: 0,
        name: SharedString::from("Console"),
        is_console: true,
    }]));
    ui.set_tabs_model(ModelRc::from(tabs_model.clone()));
    ui.set_active_tab_index(0);
    ui.set_can_restore_tab(false);

    let terminals_model = Rc::new(VecModel::<TerminalInfo>::from(vec![TerminalInfo {
        index: 0,
        id: SharedString::from(""),
        label: SharedString::from("."),
        cwd: SharedString::from(""),
        running: true,
    }]));
    ui.set_terminals_model(ModelRc::from(terminals_model.clone()));
    ui.set_active_terminal_index(0);

    let filters_model = Rc::new(VecModel::<FilterInfo>::from(vec![]));
    ui.set_filters_model(ModelRc::from(filters_model.clone()));
    ui.set_filter_draft(SharedString::default());
    ui.set_filter_draft_regex(false);
    ui.set_filters_editable(false);
    ui.set_find_open(false);
    ui.set_find_query(SharedString::default());
    ui.set_find_case_sensitive(false);
    ui.set_find_whole_word(false);
    ui.set_find_regex(false);
    ui.set_find_status(SharedString::default());
    ui.set_find_error(SharedString::default());
    ui.set_max_scrollback_lines(DEFAULT_MAX_SCROLLBACK_LINES as i32);
    // Opaque placeholder before `ui.run()` — empty Image punches a see-through hole.
    seed_opaque_viewport(&ui);

    let cli: Vec<String> = std::env::args().skip(1).collect();
    let launch = launch_args::parse(&cli);
    if launch.has_process_launch() {
        let label = launch
            .command
            .as_deref()
            .or(launch.log_file.as_deref())
            .unwrap_or("launch");
        ui.set_status_text(SharedString::from(format!("launch: {label}")));
    } else {
        ui.set_status_text(SharedString::from("interactive shell"));
    }
    let mut engine = Engine::new();
    engine.set_launch(launch);

    let engine = Rc::new(RefCell::new(engine));
    let logical_size = Rc::new(RefCell::new((800.0f32, 600.0f32)));
    let force_render = Rc::new(Cell::new(true));
    let console_active = Rc::new(Cell::new(true));
    // Engine starts unfocused; Window forward-focus may focus the viewport and fire the callback.
    let viewport_focused = Rc::new(Cell::new(false));
    let timer = Rc::new(Timer::default());
    let timer_fast = Rc::new(Cell::new(true));
    let find_debounce = Rc::new(Timer::default());
    let find_pending = Rc::new(RefCell::new(None::<(String, bool, bool, bool)>));
    let filter_draft_debounce = Rc::new(Timer::default());
    let filter_draft_pending = Rc::new(RefCell::new(None::<(String, bool)>));
    // When true, next stats push overwrites find query/toggles (open / tab switch).
    let find_resync = Rc::new(Cell::new(false));
    let find_stats_tab = Rc::new(Cell::new(-1i32));
    // Compositor occlusion (Wayland minimize) — Slint Window::is_minimized is unreliable here.
    let window_occluded = Rc::new(Cell::new(false));
    // Last tick saw an occluded window; used to force one paint on restore.
    let was_occluded = Rc::new(Cell::new(false));
    // At least one Engine::render bitmap has been uploaded this process.
    let viewport_presented = Rc::new(Cell::new(false));

    window_chrome::install(
        &ui,
        window_occluded.clone(),
        force_render.clone(),
        timer.clone(),
        timer_fast.clone(),
        viewport_presented.clone(),
    );

    {
        let logical_size = logical_size.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let window_occluded = window_occluded.clone();
        let viewport_presented = viewport_presented.clone();
        ui.on_viewport_resized(move |width, height| {
            if width > 1.0 && height > 1.0 {
                *logical_size.borrow_mut() = (width, height);
                force_render.set(true);
                // Don't re-arm 33ms while occluded — unless we still owe the first present.
                if !window_occluded.get() || !viewport_presented.get() {
                    bump_fast_timer(&timer, &timer_fast);
                }
            }
        });
    }

    {
        let engine = engine.clone();
        let viewport_focused = viewport_focused.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let force_render = force_render.clone();
        let ui_focus = ui.as_weak();
        let logical_size = logical_size.clone();
        ui.on_viewport_focused(move |focused| {
            viewport_focused.set(focused);
            let mut eng = engine.borrow_mut();
            let _ = eng.send_command(Command::SetViewportFocus { focused });
            if focused {
                eng.reset_caret_blink();
                force_render.set(true);
                bump_fast_timer(&timer, &timer_fast);
                if let Some(ui) = ui_focus.upgrade() {
                    ui.set_caret_blink_on(true);
                    let (lw, lh) = *logical_size.borrow();
                    let scale = ui.window().scale_factor().max(0.5) as f32;
                    let width = (lw * scale).ceil().max(1.0) as u32;
                    let height = (lh * scale).ceil().max(1.0) as u32;
                    let _ = sync_console_caret(&ui, &eng, width, height, scale);
                }
            } else if let Some(ui) = ui_focus.upgrade() {
                ui.set_caret_visible(false);
            }
        });
    }

    let syncing_scroll = Rc::new(Cell::new(false));
    let syncing_follow = Rc::new(Cell::new(false));
    let has_selection = Rc::new(Cell::new(false));
    let pty_running = Rc::new(Cell::new(true));
    let selecting = Rc::new(Cell::new(false));
    let click_tracker = Rc::new(RefCell::new(ClickTracker::new()));
    ui.set_can_copy(false);
    ui.set_can_paste(false);
    // Engine tabs default wrap_lines: true — match app.slint / menu checkmarks.
    // Ongoing sync: apply_stats from typed StatsSnapshot.
    ui.set_wrap_lines(true);
    // Engine auto_follow ↔ Slint auto-follow / set-follow callback.
    ui.set_auto_follow(true);
    ui.set_can_close_tab(false);
    ui.set_can_restore_tab(false);
    // Startup active tab is Console — Rename disabled until a filter tab is active.
    ui.set_can_rename_tab(false);
    // Explicit idle — empty renaming-terminal-id must not match placeholder term.id "".
    ui.set_renaming_tab_index(-1);
    ui.set_renaming_terminal_id(SharedString::default());
    ui.set_rename_draft(SharedString::default());
    let viewport_font_size = Rc::new(Cell::new(13.0_f32));

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let viewport_font_size = viewport_font_size.clone();
        ui.on_zoom_in(move || {
            let next = (viewport_font_size.get() + 1.0).clamp(8.0, 32.0);
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetViewportFontSize { size: next });
            viewport_font_size.set(next);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let viewport_font_size = viewport_font_size.clone();
        ui.on_zoom_out(move || {
            let next = (viewport_font_size.get() - 1.0).clamp(8.0, 32.0);
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetViewportFontSize { size: next });
            viewport_font_size.set(next);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let viewport_font_size = viewport_font_size.clone();
        ui.on_zoom_reset(move || {
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetViewportFontSize { size: 13.0 });
            viewport_font_size.set(13.0);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let viewport_font_size = viewport_font_size.clone();
        ui.on_viewport_zoom_wheel(move |delta_y| {
            let step = if delta_y > 0.0 { 1.0 } else { -1.0 };
            let next = (viewport_font_size.get() + step).clamp(8.0, 32.0);
            if (next - viewport_font_size.get()).abs() < f32::EPSILON {
                return;
            }
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetViewportFontSize { size: next });
            viewport_font_size.set(next);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_wrap = ui.as_weak();
        ui.on_set_wrap_lines(move |wrap| {
            if let Some(ui) = ui_wrap.upgrade() {
                ui.set_wrap_lines(wrap);
            }
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetWrapLines { wrap });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let syncing_follow = syncing_follow.clone();
        let ui_follow = ui.as_weak();
        // Slint set-follow → engine SetFollow; stats field is auto_follow / property auto-follow.
        ui.on_set_follow(move |follow| {
            if syncing_follow.get() {
                return;
            }
            if let Some(ui) = ui_follow.upgrade() {
                ui.set_auto_follow(follow);
            }
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetFollow { follow });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_sev = ui.as_weak();
        ui.on_set_severity(move |mode| {
            let mode_str = mode.to_string();
            if let Some(ui) = ui_sev.upgrade() {
                ui.set_severity_mode(mode.clone());
            }
            let _ = engine
                .borrow_mut()
                .send_command(Command::SeveritySet { mode: mode_str });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_records_expand_all(move || {
            let _ = engine.borrow_mut().send_command(Command::RecordsExpandAll);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_records_collapse_all(move || {
            let _ = engine
                .borrow_mut()
                .send_command(Command::RecordsCollapseAll);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_viewport_scrolled(move |delta_y| {
            let lines = if delta_y > 0.0 { -3 } else { 3 };
            let _ = engine
                .borrow_mut()
                .send_command(Command::ScrollLines { delta: lines });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_viewport_scrolled_x(move |delta_x| {
            // Logical-ish step; engine clamps to max_scroll_x.
            let delta = if delta_x > 0.0 { -40.0 } else { 40.0 };
            let _ = engine
                .borrow_mut()
                .send_command(Command::ScrollHorizontal { delta });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let syncing_scroll = syncing_scroll.clone();
        ui.on_viewport_scroll_y_changed(move |value| {
            if syncing_scroll.get() {
                return;
            }
            let _ = engine
                .borrow_mut()
                .send_command(Command::Scroll { offset: value });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let syncing_scroll = syncing_scroll.clone();
        ui.on_viewport_scroll_x_changed(move |value| {
            if syncing_scroll.get() {
                return;
            }
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetScrollX { offset: value });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let console_active = console_active.clone();
        let selecting = selecting.clone();
        let has_selection = has_selection.clone();
        let click_tracker = click_tracker.clone();
        let ui_ptr = ui.as_weak();
        ui.on_viewport_pointer(move |x, y, button, kind| {
            let Some(ui) = ui_ptr.upgrade() else {
                return;
            };
            let scale = ui.window().scale_factor().max(0.5) as f32;
            let px = x * scale;
            let py = y * scale;

            // kind: 0=down, 1=up, 2=move; button: 0=left, 1=middle, 2=right
            if kind == 0 && button == 1 {
                // Middle-click paste (console only).
                if console_active.get() {
                    paste_clipboard_to_console(&mut engine.borrow_mut());
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                }
                return;
            }

            if button != 0 {
                return;
            }

            match kind {
                0 => {
                    selecting.set(true);
                    let click_count = click_tracker.borrow_mut().on_press(px, py);
                    let _ = engine.borrow_mut().send_command(Command::SelectionAt {
                        x: px,
                        y: py,
                        extend: false,
                        click_count,
                    });
                    let selected = engine.borrow().selection_text().is_some();
                    has_selection.set(selected);
                    ui.set_can_copy(selected);
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                }
                2 => {
                    if !selecting.get() {
                        return;
                    }
                    let _ = engine.borrow_mut().send_command(Command::SelectionAt {
                        x: px,
                        y: py,
                        extend: true,
                        click_count: 1,
                    });
                    let selected = engine.borrow().selection_text().is_some();
                    has_selection.set(selected);
                    ui.set_can_copy(selected);
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                }
                1 => {
                    selecting.set(false);
                }
                _ => {}
            }
        });
    }

    {
        let engine = engine.clone();
        let console_active = console_active.clone();
        let has_selection = has_selection.clone();
        let pty_running = pty_running.clone();
        let ui_ptr = ui.as_weak();
        ui.on_viewport_context_opening(move || {
            let Some(ui) = ui_ptr.upgrade() else {
                return;
            };
            // Viewport context menu: Copy from selection,
            // Paste when console + running + clipboard has text.
            let selected = has_selection.get()
                || engine.borrow().selection_text().is_some_and(|t| !t.is_empty());
            has_selection.set(selected);
            ui.set_can_copy(selected);
            let can_paste = console_active.get() && pty_running.get() && clipboard_has_text();
            ui.set_can_paste(can_paste);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_viewport_copy(move || {
            if copy_selection_to_clipboard(&engine.borrow()) {
                force_render.set(true);
                bump_fast_timer(&timer, &timer_fast);
            }
        });
    }

    {
        let engine = engine.clone();
        let console_active = console_active.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_viewport_paste(move || {
            if !console_active.get() {
                return;
            }
            paste_clipboard_to_console(&mut engine.borrow_mut());
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let console_active = console_active.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let has_selection = has_selection.clone();
        let viewport_font_size = viewport_font_size.clone();
        let ui_find_key = ui.as_weak();
        let find_resync_key = find_resync.clone();
        ui.on_key_event(move |text, control, meta, _alt, shift| {
            let ctrl = control || meta;
            // Ctrl/Cmd+F → open/focus find (never send 0x06 to PTY).
            if ctrl && is_key_char(&text, 'f') {
                if let Some(ui) = ui_find_key.upgrade() {
                    let was_open = ui.get_find_open();
                    ui.set_find_open(true);
                    ui.set_find_focus_request(true);
                    // Resync from engine only when opening — not on every re-focus,
                    // or a pending typed query can be overwritten by a stale empty search.
                    if !was_open {
                        find_resync_key.set(true);
                        // Closing Find clears engine search; re-apply the last query
                        // so highlights return with the bar.
                        let query = ui.get_find_query();
                        if !query.is_empty() {
                            let _ = engine.borrow_mut().send_command(Command::SearchSet {
                                query: query.to_string(),
                                regex: ui.get_find_regex(),
                                case_sensitive: ui.get_find_case_sensitive(),
                                whole_word: ui.get_find_whole_word(),
                            });
                        }
                    }
                }
                force_render.set(true);
                bump_fast_timer(&timer, &timer_fast);
                return true;
            }
            // Escape closes find when open and clears engine search.
            if text == "\u{1b}" {
                if let Some(ui) = ui_find_key.upgrade() {
                    if ui.get_find_open() {
                        ui.set_find_open(false);
                        ui.invoke_find_closed();
                        force_render.set(true);
                        bump_fast_timer(&timer, &timer_fast);
                        return true;
                    }
                }
            }
            if ctrl {
                if is_zoom_in_key(&text) {
                    let next = (viewport_font_size.get() + 1.0).clamp(8.0, 32.0);
                    let _ = engine
                        .borrow_mut()
                        .send_command(Command::SetViewportFontSize { size: next });
                    viewport_font_size.set(next);
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                    return true;
                }
                if text == "-" {
                    let next = (viewport_font_size.get() - 1.0).clamp(8.0, 32.0);
                    let _ = engine
                        .borrow_mut()
                        .send_command(Command::SetViewportFontSize { size: next });
                    viewport_font_size.set(next);
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                    return true;
                }
                if text == "0" {
                    let _ = engine
                        .borrow_mut()
                        .send_command(Command::SetViewportFontSize { size: 13.0 });
                    viewport_font_size.set(13.0);
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                    return true;
                }
            }
            // Copy: Ctrl/Meta+C with an active selection (do not send SIGINT).
            if ctrl && is_key_char(&text, 'c') && has_selection.get() {
                if copy_selection_to_clipboard(&engine.borrow()) {
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                    return true;
                }
            }
            // Paste: Ctrl/Meta+V or Shift+Insert (console only).
            let insert = text == "\u{f727}";
            if console_active.get()
                && ((ctrl && is_key_char(&text, 'v')) || (shift && insert))
            {
                paste_clipboard_to_console(&mut engine.borrow_mut());
                force_render.set(true);
                bump_fast_timer(&timer, &timer_fast);
                return true;
            }
            if !console_active.get() {
                // Still allow copy from filter tabs.
                if ctrl && is_key_char(&text, 'c') && has_selection.get() {
                    let _ = copy_selection_to_clipboard(&engine.borrow());
                    return true;
                }
                return true;
            }
            // Do not force_render before echo — paint when PTY content dirties.
            bump_fast_timer(&timer, &timer_fast);
            if let Some(ui) = ui_find_key.upgrade() {
                ui.set_caret_blink_on(true);
            }
            handle_key_event(&mut engine.borrow_mut(), &text, ctrl)
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let console_active = console_active.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let logical_size = logical_size.clone();
        let viewport_focused = viewport_focused.clone();
        let ui_tabs = ui.as_weak();
        ui.on_tab_switch(move |index| {
            if let Some(ui) = ui_tabs.upgrade() {
                ui.set_active_tab_index(index);
                ui.set_filters_editable(index != 0);
            }
            console_active.set(index == 0);
            let mut eng = engine.borrow_mut();
            let _ = eng.send_command(Command::TabSwitch {
                index: index as usize,
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
            if let Some(ui) = ui_tabs.upgrade() {
                if index == 0 {
                    viewport_focused.set(true);
                    arm_console_caret(&ui, &mut eng, *logical_size.borrow());
                } else {
                    ui.set_caret_visible(false);
                }
            }
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_tab_move(move |from_index, to_index| {
            if from_index < 0 || to_index < 0 {
                return;
            }
            let _ = engine.borrow_mut().send_command(Command::TabMove {
                from_index: from_index as usize,
                to_index: to_index as usize,
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let console_active = console_active.clone();
        let tabs_model = tabs_model.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_tabs = ui.as_weak();
        ui.on_tab_add(move || {
            let _ = engine.borrow_mut().send_command(Command::TabAdd);
            let next_index = tabs_model.row_count() as i32;
            let name = SharedString::from(format!("Tab {}", next_index + 1));
            tabs_model.push(TabInfo {
                index: next_index,
                name,
                is_console: false,
            });
            if let Some(ui) = ui_tabs.upgrade() {
                ui.set_active_tab_index(next_index);
                ui.set_filters_editable(true);
                ui.set_filter_draft(SharedString::default());
            }
            console_active.set(false);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let console_active = console_active.clone();
        let tabs_model = tabs_model.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_tabs = ui.as_weak();
        ui.on_tab_close(move |index| {
            if index <= 0 {
                return;
            }
            let old_active = ui_tabs
                .upgrade()
                .map(|u| u.get_active_tab_index())
                .unwrap_or(0);
            let _ = engine
                .borrow_mut()
                .send_command(Command::TabClose {
                    index: index as usize,
                });
            let row = index as usize;
            if row < tabs_model.row_count() {
                tabs_model.remove(row);
                for i in row..tabs_model.row_count() {
                    if let Some(mut t) = tabs_model.row_data(i) {
                        t.index = i as i32;
                        tabs_model.set_row_data(i, t);
                    }
                }
            }
            let max = (tabs_model.row_count().saturating_sub(1)) as i32;
            let new_active = if index < old_active {
                old_active - 1
            } else if index == old_active {
                index.min(max)
            } else {
                old_active
            };
            if let Some(ui) = ui_tabs.upgrade() {
                ui.set_active_tab_index(new_active);
                ui.set_can_restore_tab(true);
                ui.set_filters_editable(new_active != 0);
            }
            console_active.set(new_active == 0);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let console_active = console_active.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_tabs = ui.as_weak();
        ui.on_tab_restore(move || {
            let _ = engine.borrow_mut().send_command(Command::TabRestore);
            console_active.set(false);
            if let Some(ui) = ui_tabs.upgrade() {
                ui.set_filters_editable(true);
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let tabs_model = tabs_model.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_tab_rename(move |index, name| {
            let name = name.trim();
            if name.is_empty() || index < 0 {
                return;
            }
            let _ = engine.borrow_mut().send_command(Command::TabRename {
                index: index as usize,
                name: name.to_string(),
            });
            let row = index as usize;
            if row < tabs_model.row_count() {
                if let Some(mut t) = tabs_model.row_data(row) {
                    t.name = SharedString::from(name);
                    tabs_model.set_row_data(row, t);
                }
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let terminals_model = terminals_model.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_term = ui.as_weak();
        ui.on_terminal_switch(move |id| {
            let id_str = id.as_str();
            if let Some(idx) = find_terminal_index(&terminals_model, id_str) {
                if let Some(ui) = ui_term.upgrade() {
                    ui.set_active_terminal_index(idx);
                }
            }
            let _ = engine.borrow_mut().send_command(Command::TerminalSwitch {
                terminal_id: id_str.to_string(),
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_terminal_move(move |id, to_index| {
            if to_index < 0 {
                return;
            }
            let _ = engine.borrow_mut().send_command(Command::TerminalMove {
                terminal_id: id.as_str().to_string(),
                to_index: to_index as usize,
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let terminals_model = terminals_model.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_terminal_rename(move |id, name| {
            let name = name.trim();
            if name.is_empty() || id.is_empty() {
                return;
            }
            let id_str = id.as_str();
            let _ = engine.borrow_mut().send_command(Command::TerminalRename {
                terminal_id: id_str.to_string(),
                name: name.to_string(),
            });
            if let Some(row) = find_terminal_index(&terminals_model, id_str).map(|i| i as usize) {
                if let Some(mut t) = terminals_model.row_data(row) {
                    t.label = SharedString::from(name);
                    terminals_model.set_row_data(row, t);
                }
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_term = ui.as_weak();
        let terminals_model = terminals_model.clone();
        ui.on_terminal_add(move || {
            let _ = engine.borrow_mut().send_command(Command::TerminalAdd);
            // Optimistic highlight; full row (with id) comes from immediate stats flush.
            let next = terminals_model.row_count() as i32;
            if let Some(ui) = ui_term.upgrade() {
                ui.set_active_terminal_index(next);
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let terminals_model = terminals_model.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_term = ui.as_weak();
        ui.on_terminal_close(move |id| {
            let id_str = id.as_str();
            let Some(row) = find_terminal_index(&terminals_model, id_str).map(|i| i as usize) else {
                return;
            };
            if row == 0 || terminals_model.row_count() <= 1 {
                return;
            }
            let old_active = ui_term
                .upgrade()
                .map(|u| u.get_active_terminal_index())
                .unwrap_or(0);
            let _ = engine.borrow_mut().send_command(Command::TerminalClose {
                terminal_id: Some(id_str.to_string()),
            });

            terminals_model.remove(row);
            for i in row..terminals_model.row_count() {
                if let Some(mut t) = terminals_model.row_data(i) {
                    t.index = i as i32;
                    terminals_model.set_row_data(i, t);
                }
            }
            let index = row as i32;
            let max = (terminals_model.row_count().saturating_sub(1)) as i32;
            let new_active = if index < old_active {
                old_active - 1
            } else if index == old_active {
                index.min(max)
            } else {
                old_active
            };
            if let Some(ui) = ui_term.upgrade() {
                ui.set_active_terminal_index(new_active);
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_filt = ui.as_weak();
        let filter_draft_debounce = filter_draft_debounce.clone();
        let filter_draft_pending = filter_draft_pending.clone();
        ui.on_filter_add(move |filter_type, pattern, use_regex| {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                return;
            }
            let filter_type = match filter_type.as_str() {
                "exclude" => FilterType::Exclude,
                _ => FilterType::Include,
            };
            let _ = engine.borrow_mut().send_command(Command::FilterAdd {
                filter_type,
                pattern: pattern.to_string(),
                regex: use_regex,
            });
            // Clear draft preview immediately (UI also notifies filter-draft-changed).
            filter_draft_debounce.stop();
            *filter_draft_pending.borrow_mut() = None;
            let _ = engine.borrow_mut().send_command(Command::FilterDraftSet {
                pattern: String::new(),
                use_regex: false,
            });
            if let Some(ui) = ui_filt.upgrade() {
                ui.set_filter_draft(SharedString::default());
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let filter_draft_debounce = filter_draft_debounce.clone();
        let filter_draft_pending = filter_draft_pending.clone();
        ui.on_filter_draft_changed(move |pattern, use_regex| {
            *filter_draft_pending.borrow_mut() = Some((pattern.to_string(), use_regex));
            let engine = engine.clone();
            let force_render = force_render.clone();
            let timer = timer.clone();
            let timer_fast = timer_fast.clone();
            let filter_draft_pending = filter_draft_pending.clone();
            filter_draft_debounce.start(
                TimerMode::SingleShot,
                FILTER_DRAFT_DEBOUNCE,
                move || {
                    let Some((pattern, use_regex)) = filter_draft_pending.borrow_mut().take()
                    else {
                        return;
                    };
                    let _ = engine.borrow_mut().send_command(Command::FilterDraftSet {
                        pattern,
                        use_regex,
                    });
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                },
            );
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let find_debounce = find_debounce.clone();
        let find_pending = find_pending.clone();
        ui.on_find_query_changed(move |query, regex, case_sensitive, whole_word| {
            *find_pending.borrow_mut() = Some((
                query.to_string(),
                regex,
                case_sensitive,
                whole_word,
            ));
            let engine = engine.clone();
            let force_render = force_render.clone();
            let timer = timer.clone();
            let timer_fast = timer_fast.clone();
            let find_pending = find_pending.clone();
            find_debounce.start(TimerMode::SingleShot, FIND_DEBOUNCE, move || {
                let Some((q, re, cs, ww)) = find_pending.borrow_mut().take() else {
                    return;
                };
                let _ = engine.borrow_mut().send_command(Command::SearchSet {
                    query: q,
                    regex: re,
                    case_sensitive: cs,
                    whole_word: ww,
                });
                force_render.set(true);
                bump_fast_timer(&timer, &timer_fast);
            });
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let find_debounce = find_debounce.clone();
        let find_pending = find_pending.clone();
        ui.on_find_goto(move |delta| {
            // Flush pending search_set before navigating.
            find_debounce.stop();
            if let Some((q, re, cs, ww)) = find_pending.borrow_mut().take() {
                let _ = engine.borrow_mut().send_command(Command::SearchSet {
                    query: q,
                    regex: re,
                    case_sensitive: cs,
                    whole_word: ww,
                });
            }
            let _ = engine
                .borrow_mut()
                .send_command(Command::SearchGoto { delta });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let find_debounce = find_debounce.clone();
        let find_pending = find_pending.clone();
        ui.on_find_commit(move || {
            find_debounce.stop();
            let Some((q, re, cs, ww)) = find_pending.borrow_mut().take() else {
                return;
            };
            let _ = engine.borrow_mut().send_command(Command::SearchSet {
                query: q,
                regex: re,
                case_sensitive: cs,
                whole_word: ww,
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let find_debounce = find_debounce.clone();
        let find_pending = find_pending.clone();
        let ui_closed = ui.as_weak();
        ui.on_find_closed(move || {
            // Drop in-flight typing so a late debounce cannot re-apply search.
            find_debounce.stop();
            find_pending.borrow_mut().take();
            let (regex, case_sensitive, whole_word) = ui_closed
                .upgrade()
                .map(|ui| {
                    (
                        ui.get_find_regex(),
                        ui.get_find_case_sensitive(),
                        ui.get_find_whole_word(),
                    )
                })
                .unwrap_or((false, false, false));
            let _ = engine.borrow_mut().send_command(Command::SearchSet {
                query: String::new(),
                regex,
                case_sensitive,
                whole_word,
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_filter_toggle(move |id, enabled| {
            let id = id.as_str();
            if id.is_empty() {
                return;
            }
            let _ = engine.borrow_mut().send_command(Command::FilterToggle {
                id: id.to_string(),
                enabled,
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_filter_remove(move |id| {
            let id = id.as_str();
            if id.is_empty() {
                return;
            }
            let _ = engine.borrow_mut().send_command(Command::FilterRemove {
                id: id.to_string(),
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_filter_update(move |id, pattern| {
            let id = id.as_str();
            let pattern = pattern.trim();
            if id.is_empty() || pattern.is_empty() {
                return;
            }
            let _ = engine.borrow_mut().send_command(Command::FilterUpdate {
                id: id.to_string(),
                pattern: pattern.to_string(),
            });
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        ui.on_filter_clear(move || {
            let _ = engine.borrow_mut().send_command(Command::FilterClear);
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_open = ui.as_weak();
        ui.on_open_log_file(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Open log file")
                .add_filter("Log files", &["log", "txt", "out", "json", "jsonl"])
                .add_filter("All files", &["*"])
                .pick_file();
            let Some(path) = picked else {
                return;
            };
            let path_str = path.to_string_lossy();
            if path_str.is_empty() {
                if let Some(ui) = ui_open.upgrade() {
                    ui.set_status_text(SharedString::from("open log: empty path"));
                }
                return;
            }
            if let Err(err) = engine.borrow_mut().send_command(Command::LoadFile {
                path: path_str.into_owned(),
            }) {
                if let Some(ui) = ui_open.upgrade() {
                    ui.set_status_text(SharedString::from(format!("open log: {err}")));
                }
                return;
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    {
        let engine = engine.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let ui_set = ui.as_weak();
        ui.on_settings_apply(move |value| {
            let raw = if value < 0 { 0usize } else { value as usize };
            let capped = clamp_max_scrollback_lines(raw);
            let _ = engine
                .borrow_mut()
                .send_command(Command::SetSettings {
                    max_scrollback_lines: capped,
                });
            if let Some(ui) = ui_set.upgrade() {
                ui.set_max_scrollback_lines(capped as i32);
            }
            force_render.set(true);
            bump_fast_timer(&timer, &timer_fast);
        });
    }

    let ui_weak = ui.as_weak();
    let engine_tick = engine.clone();
    let logical_tick = logical_size.clone();
    let force_tick = force_render.clone();
    let tabs_tick = tabs_model.clone();
    let terminals_tick = terminals_model.clone();
    let filters_tick = filters_model.clone();
    let console_tick = console_active.clone();
    let timer_tick = timer.clone();
    let timer_fast_tick = timer_fast.clone();
    let window_occluded_tick = window_occluded.clone();
    let was_occluded_tick = was_occluded.clone();
    let presented_tick = viewport_presented.clone();
    let syncing_scroll_tick = syncing_scroll.clone();
    let syncing_follow_tick = syncing_follow.clone();
    let has_selection_tick = has_selection.clone();
    let pty_running_tick = pty_running.clone();
    let viewport_font_size_tick = viewport_font_size.clone();
    let find_resync_tick = find_resync.clone();
    let find_pending_tick = find_pending.clone();
    let find_debounce_tick = find_debounce.clone();
    let find_stats_tab_tick = find_stats_tab.clone();

    // Shared tick body so the PTY reader can wake the UI without waiting for TICK_FAST.
    let ticking = Arc::new(AtomicBool::new(false));
    let needs_retick = Arc::new(AtomicBool::new(false));

    {
        let ticking = ticking.clone();
        let needs_retick = needs_retick.clone();
        let ui_weak = ui_weak.clone();
        let engine_tick = engine_tick.clone();
        let logical_tick = logical_tick.clone();
        let force_tick = force_tick.clone();
        let tabs_tick = tabs_tick.clone();
        let terminals_tick = terminals_tick.clone();
        let filters_tick = filters_tick.clone();
        let console_tick = console_tick.clone();
        let timer_tick = timer_tick.clone();
        let timer_fast_tick = timer_fast_tick.clone();
        let window_occluded_tick = window_occluded_tick.clone();
        let was_occluded_tick = was_occluded_tick.clone();
        let presented_tick = presented_tick.clone();
        let syncing_scroll_tick = syncing_scroll_tick.clone();
        let syncing_follow_tick = syncing_follow_tick.clone();
        let has_selection_tick = has_selection_tick.clone();
        let pty_running_tick = pty_running_tick.clone();
        let viewport_font_size_tick = viewport_font_size_tick.clone();
        let find_resync_tick = find_resync_tick.clone();
        let find_pending_tick = find_pending_tick.clone();
        let find_debounce_tick = find_debounce_tick.clone();
        let find_stats_tab_tick = find_stats_tab_tick.clone();

        HOST_TICK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                if ticking.swap(true, Ordering::AcqRel) {
                    needs_retick.store(true, Ordering::Release);
                    return;
                }
                loop {
                    needs_retick.store(false, Ordering::Release);

                    let Some(ui) = ui_weak.upgrade() else {
                        break;
                    };

                    let occluded = window_should_pause_paint(
                        ui.window(),
                        window_occluded_tick.get(),
                        presented_tick.get(),
                    );

                    let became_occluded = occluded && !was_occluded_tick.get();
                    if was_occluded_tick.get() && !occluded {
                        force_tick.set(true);
                    }
                    was_occluded_tick.set(occluded);

                    let mut eng = engine_tick.borrow_mut();
                    eng.tick();

                    if occluded {
                        while eng.poll_event_json().is_some() {}
                        if became_occluded || timer_fast_tick.get() {
                            set_occluded_timer(&timer_tick, &timer_fast_tick);
                        }
                        drop(eng);
                        if !needs_retick.load(Ordering::Acquire) {
                            break;
                        }
                        continue;
                    }

                    while let Some(ev) = eng.poll_event_json() {
                        match parse_engine_event(&ev) {
                            Some(EngineEvent::Stats(stats)) => {
                                if apply_stats(
                                    &stats,
                                    &tabs_tick,
                                    &terminals_tick,
                                    &filters_tick,
                                    &ui,
                                    &console_tick,
                                    &syncing_scroll_tick,
                                    &has_selection_tick,
                                    &pty_running_tick,
                                    &viewport_font_size_tick,
                                    &syncing_follow_tick,
                                    &find_resync_tick,
                                    &find_stats_tab_tick,
                                    &find_pending_tick,
                                    &find_debounce_tick,
                                ) {
                                    force_tick.set(true);
                                }
                            }
                            Some(EngineEvent::Status { message }) => {
                                ui.set_status_text(SharedString::from(message));
                            }
                            Some(EngineEvent::Exit { code, .. }) => {
                                ui.set_status_text(SharedString::from(format!("exit {code}")));
                            }
                            _ => {}
                        }
                    }

                    let dirty = eng.needs_render() || force_tick.get();
                    if dirty {
                        bump_fast_timer(&timer_tick, &timer_fast_tick);
                    } else if timer_fast_tick.get() {
                        timer_tick.set_interval(TICK_IDLE);
                        timer_fast_tick.set(false);
                    }

                    let (lw, lh) = *logical_tick.borrow();
                    if lw <= 1.0 || lh <= 1.0 {
                        drop(eng);
                        if !needs_retick.load(Ordering::Acquire) {
                            break;
                        }
                        continue;
                    }
                    let scale = ui.window().scale_factor().max(0.5) as f32;
                    let width = (lw * scale).ceil().max(1.0) as u32;
                    let height = (lh * scale).ceil().max(1.0) as u32;
                    ui.set_viewport_page_w(width as f32);
                    ui.set_viewport_page_h(height as f32);

                    // Caret overlay tracks focus/tab/running even when the Image is idle.
                    let was_visible = ui.get_caret_visible();
                    let now_visible = sync_console_caret(&ui, &eng, width, height, scale);
                    // Shell often becomes ready after first focus — re-arm blink when caret appears.
                    if now_visible && !was_visible {
                        ui.set_caret_blink_on(true);
                    }

                    if !dirty {
                        drop(eng);
                        if !needs_retick.load(Ordering::Acquire) {
                            break;
                        }
                        continue;
                    }
                    force_tick.set(false);

                    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
                    if let Err(err) = eng.render(width, height, buffer.make_mut_bytes()) {
                        ui.set_status_text(SharedString::from(format!("render: {err}")));
                        drop(eng);
                        break;
                    }
                    // Position may change with scroll/follow after paint.
                    let was_visible = ui.get_caret_visible();
                    let now_visible = sync_console_caret(&ui, &eng, width, height, scale);
                    if now_visible && !was_visible {
                        ui.set_caret_blink_on(true);
                    }
                    drop(eng);
                    ui.set_viewport_image(Image::from_rgba8(buffer));
                    if !presented_tick.get() {
                        presented_tick.set(true);
                        ui.window().request_redraw();
                    }

                    if !needs_retick.load(Ordering::Acquire) {
                        break;
                    }
                }
                ticking.store(false, Ordering::Release);
            }));
        });
    }

    {
        let ticking = ticking.clone();
        let needs_retick = needs_retick.clone();
        let pending = Arc::new(AtomicBool::new(false));
        engine.borrow_mut().set_pty_activity_wake(Arc::new(move || {
            if ticking.load(Ordering::Acquire) {
                needs_retick.store(true, Ordering::Release);
                return;
            }
            if pending.swap(true, Ordering::AcqRel) {
                return;
            }
            let pending = pending.clone();
            let _ = slint::invoke_from_event_loop(move || {
                pending.store(false, Ordering::Release);
                HOST_TICK.with(|slot| {
                    if let Some(f) = slot.borrow_mut().as_mut() {
                        f();
                    }
                });
            });
        }));
    }

    timer.start(TimerMode::Repeated, TICK_FAST, move || {
        HOST_TICK.with(|slot| {
            if let Some(f) = slot.borrow_mut().as_mut() {
                f();
            }
        });
    });

    // Blink only flips overlay opacity — never re-rasters the log Image.
    let caret_blink_timer = Rc::new(Timer::default());
    {
        let ui_blink = ui.as_weak();
        caret_blink_timer.start(TimerMode::Repeated, CARET_BLINK_PERIOD, move || {
            if let Some(ui) = ui_blink.upgrade() {
                if ui.get_caret_visible() {
                    ui.set_caret_blink_on(!ui.get_caret_blink_on());
                }
            }
        });
    }

    // Init / forward-focus can fire before Rust handlers exist, so engine never
    // learns viewport_focused=true until a later click. Arm caret after the loop starts.
    {
        let ui_boot = ui.as_weak();
        let engine = engine.clone();
        let logical_size = logical_size.clone();
        let viewport_focused = viewport_focused.clone();
        let force_render = force_render.clone();
        let timer = timer.clone();
        let timer_fast = timer_fast.clone();
        let schedule = |delay_ms: u64| {
            let ui_boot = ui_boot.clone();
            let engine = engine.clone();
            let logical_size = logical_size.clone();
            let viewport_focused = viewport_focused.clone();
            let force_render = force_render.clone();
            let timer = timer.clone();
            let timer_fast = timer_fast.clone();
            Timer::single_shot(Duration::from_millis(delay_ms), move || {
                let Some(ui) = ui_boot.upgrade() else {
                    return;
                };
                if ui.get_active_tab_index() != 0 {
                    return;
                }
                viewport_focused.set(true);
                let mut eng = engine.borrow_mut();
                arm_console_caret(&ui, &mut eng, *logical_size.borrow());
                force_render.set(true);
                bump_fast_timer(&timer, &timer_fast);
            });
        };
        schedule(0);
        // Shell prompt / live screen may appear slightly after first paint.
        schedule(150);
    }

    // Keep Rc<Timer> alive for the UI lifetime (Drop stops the timer).
    let _tick_timer = timer;
    let _caret_blink_timer = caret_blink_timer;
    let _find_debounce = find_debounce;
    let _filter_draft_debounce = filter_draft_debounce;

    ui.run()?;
    Ok(())
}
