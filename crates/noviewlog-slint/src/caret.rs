//! Caret overlay helpers: engine geometry → Slint overlay, focus arming,
//! blink timer, and post-loop boot arming.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use noviewlog_core::{CARET_BLINK_PERIOD, Command, Engine};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::ctx::Ctx;
use noviewlog_slint::ui::AppWindow;

/// Sync Slint caret overlay from engine geometry (device px → logical).
/// Returns whether the overlay is shown.
pub(crate) fn sync_terminal_caret(ui: &AppWindow, eng: &Engine, width: u32, height: u32, scale: f32) -> bool {
    if !eng.terminal_caret_active() {
        ui.set_caret_visible(false);
        return false;
    }
    let Some((x, y, w, h)) = eng.terminal_caret_rect(width, height) else {
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

/// Focus Terminal tab viewport + engine flag + overlay (startup / tab switch).
pub(crate) fn arm_terminal_caret(ui: &AppWindow, eng: &mut Engine, logical: (f32, f32)) {
    ui.invoke_focus_viewport();
    let _ = eng.send_command(Command::SetViewportFocus { focused: true });
    eng.reset_caret_blink();
    ui.set_caret_blink_on(true);
    let scale = ui.window().scale_factor().max(0.5) as f32;
    let width = (logical.0 * scale).ceil().max(1.0) as u32;
    let height = (logical.1 * scale).ceil().max(1.0) as u32;
    let _ = sync_terminal_caret(ui, eng, width, height, scale);
}

/// Blink only flips overlay opacity — never re-rasters the log Image.
/// The returned handle must stay alive for the UI lifetime (Drop stops the timer).
pub(crate) fn start_blink_timer(ui: &AppWindow) -> Rc<Timer> {
    let timer = Rc::new(Timer::default());
    let ui_blink = ui.as_weak();
    timer.start(TimerMode::Repeated, CARET_BLINK_PERIOD, move || {
        if let Some(ui) = ui_blink.upgrade() {
            if ui.get_caret_visible() {
                ui.set_caret_blink_on(!ui.get_caret_blink_on());
            }
        }
    });
    timer
}

/// Init / forward-focus can fire before Rust handlers exist, so engine never
/// learns viewport_focused=true until a later click. Arm caret after the loop starts.
pub(crate) fn install_boot_arm(
    ui: &AppWindow,
    engine: &Rc<RefCell<Engine>>,
    logical_size: Rc<RefCell<(f32, f32)>>,
    viewport_focused: Rc<Cell<bool>>,
    ctx: &Ctx,
) {
    let ui_boot = ui.as_weak();
    let schedule = move |delay_ms: u64| {
        let ui_boot = ui_boot.clone();
        let engine = engine.clone();
        let logical_size = logical_size.clone();
        let viewport_focused = viewport_focused.clone();
        let ctx = ctx.clone();
        Timer::single_shot(Duration::from_millis(delay_ms), move || {
            let Some(ui) = ui_boot.upgrade() else {
                return;
            };
            if ui.get_active_tab_index() != 0 {
                return;
            }
            viewport_focused.set(true);
            let mut eng = engine.borrow_mut();
            arm_terminal_caret(&ui, &mut eng, *logical_size.borrow());
            ctx.refresh();
        });
    };
    schedule(0);
    // Shell prompt / live screen may appear slightly after first paint.
    schedule(150);
}
