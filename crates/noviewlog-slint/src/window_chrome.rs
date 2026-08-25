//! Winit title-drag / occlusion handler setup.

use std::cell::Cell;
use std::rc::Rc;

use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, Timer};

use crate::engine_bridge::{bump_fast_timer, set_occluded_timer};
use crate::ui::AppWindow;

/// Install title-bar drag + compositor occlusion / pointer resync handlers.
pub(crate) fn install(
    ui: &AppWindow,
    window_occluded: Rc<Cell<bool>>,
    force_render: Rc<Cell<bool>>,
    timer: Rc<Timer>,
    timer_fast: Rc<Cell<bool>>,
) {
    let window_occluded = window_occluded.clone();
    let force_render = force_render.clone();
    let timer = timer.clone();
    let timer_fast = timer_fast.clone();
    let ui_weak = ui.as_weak();
    // After drag_window(), compositor eats button-up → clear Slint grab on next move.
    let resync_pointer_after_drag = Rc::new(Cell::new(false));
    let resync_for_events = resync_pointer_after_drag.clone();
    // Last cursor (logical px) — MouseInput has no position.
    let last_cursor = Rc::new(Cell::new((0.0f32, 0.0f32)));
    // Press origin while chrome menu open + press hit title-drag gap (VS Code-like).
    let pending_title_drag = Rc::new(Cell::new(None::<(f32, f32)>));
    const TITLE_DRAG_SLOP: f32 = 4.0;

    // Title-bar drag → winit drag_window (Wayland/X11 interactive move).
    // Compositor consumes the button release, so Slint's TouchArea grab/pressed
    // stays sticky until we resync on the next CursorMoved.
    let begin_title_drag = Rc::new({
        let ui_weak = ui.as_weak();
        let resync = resync_pointer_after_drag.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let _ = ui.window().with_winit_window(|winit_window| {
                let _ = winit_window.drag_window();
            });
            // Clear immediately in case no move arrives until a click; also arm
            // CursorMoved resync so hover/click work without a priming click.
            ui.window()
                .dispatch_event(slint::platform::WindowEvent::PointerExited);
            resync.set(true);
        }
    });

    {
        let begin_title_drag = begin_title_drag.clone();
        ui.on_title_bar_drag(move || begin_title_drag());
    }

    let last_cursor_ev = last_cursor.clone();
    let pending_title_drag_ev = pending_title_drag.clone();
    let begin_title_drag_ev = begin_title_drag.clone();
    ui.window().on_winit_window_event(move |_win, event| {
        match event {
            winit::event::WindowEvent::Occluded(occluded) => {
                let was = window_occluded.get();
                window_occluded.set(*occluded);
                if *occluded {
                    // Drop to slow cadence immediately — don't wait for the next 33ms tick.
                    set_occluded_timer(&timer, &timer_fast);
                } else if was {
                    force_render.set(true);
                    bump_fast_timer(&timer, &timer_fast);
                }
            }
            winit::event::WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    // Windows / some compositors signal minimize via zero size.
                    window_occluded.set(true);
                    set_occluded_timer(&timer, &timer_fast);
                }
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let Some(ui) = ui_weak.upgrade() else {
                    return EventResult::Propagate;
                };
                let scale = ui.window().scale_factor();
                let logical = position.to_logical::<f64>(scale as f64);
                let lx = logical.x as f32;
                let ly = logical.y as f32;
                last_cursor_ev.set((lx, ly));

                if resync_for_events.get() {
                    resync_for_events.set(false);
                    // Tear down stuck press/grab, then re-enter at the real cursor.
                    ui.window()
                        .dispatch_event(slint::platform::WindowEvent::PointerExited);
                    ui.window()
                        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                            position: slint::LogicalPosition::new(lx, ly),
                        });
                }

                // Armed title-bar drag while Popup blocked the gap TouchArea.
                if let Some((px, py)) = pending_title_drag_ev.get() {
                    if (lx - px).abs() >= TITLE_DRAG_SLOP
                        || (ly - py).abs() >= TITLE_DRAG_SLOP
                    {
                        pending_title_drag_ev.set(None);
                        begin_title_drag_ev();
                    }
                }

                // PopupWindow blocks main-window hover; arm menubar via geometry hit-test.
                if ui.get_menu_bar_active() {
                    ui.invoke_menu_bar_pointer_moved(lx, ly);
                }
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                use winit::event::{ElementState, MouseButton};
                if *button != MouseButton::Left {
                    return EventResult::Propagate;
                }
                match state {
                    ElementState::Pressed => {
                        let Some(ui) = ui_weak.upgrade() else {
                            return EventResult::Propagate;
                        };
                        let (lx, ly) = last_cursor_ev.get();
                        // Popup blocks gap TouchArea — close menus + arm drag on same press.
                        if ui.get_menu_bar_active()
                            && ui.invoke_title_bar_chrome_press(lx, ly)
                        {
                            pending_title_drag_ev.set(Some((lx, ly)));
                        }
                    }
                    ElementState::Released => {
                        pending_title_drag_ev.set(None);
                    }
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
}
