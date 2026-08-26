use crate::engine::{Command, Engine};

#[test]
fn console_caret_rect_when_focused_running() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetViewportFocus { focused: true })
        .expect("focus");
    engine.mark_running_for_test();
    engine.ensure_live_screen_for_test();
    engine.rebuild_if_needed_for_test();
    let rect = engine.console_caret_rect(800, 600);
    assert!(
        rect.is_some(),
        "focused running console should expose a caret rect"
    );
    let (x, y, w, h) = rect.unwrap();
    assert!(w > 0.0 && h > 0.0, "cell size positive");
    assert!(x.is_finite() && y.is_finite());
}

#[test]
fn console_caret_rect_none_when_unfocused() {
    let mut engine = Engine::new();
    engine.mark_running_for_test();
    engine.ensure_live_screen_for_test();
    assert!(engine.console_caret_rect(800, 600).is_none());
}

#[test]
fn caret_blink_tick_does_not_dirty_viewport() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetViewportFocus { focused: true })
        .expect("focus");
    engine.mark_running_for_test();
    engine.ensure_live_screen_for_test();
    engine.rebuild_if_needed_for_test();
    let mut rgba = vec![0u8; 800 * 600 * 4];
    engine.render(800, 600, &mut rgba).expect("paint");
    assert!(!engine.needs_render());
    std::thread::sleep(std::time::Duration::from_millis(600));
    engine.tick();
    assert!(
        !engine.needs_render(),
        "engine caret blink must not dirty the bitmap viewport"
    );
}
