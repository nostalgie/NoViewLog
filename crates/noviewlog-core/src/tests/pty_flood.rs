use crate::engine::{Command, Engine, PTY_INGEST_BYTES_PER_TICK};
use crate::pty::PtyEvent;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn poll_pty_budgets_bytes_and_holds_remainder() {
    let mut engine = Engine::new();
    let id = engine.active_terminal().id.clone();
    // Ensure live VT screen exists so feed updates buffer.
    {
        let term = engine.active_terminal_mut();
        term.ingest.ensure_live_screen(&mut term.buffer);
    }

    let chunk = vec![b'a'; 4096];
    let over = (PTY_INGEST_BYTES_PER_TICK / chunk.len()) + 8;
    for _ in 0..over {
        engine
            .pty_tx
            .try_send(PtyEvent::Bytes {
                id: id.clone(),
                data: chunk.clone(),
            })
            .expect("queue has room for budget+ test");
    }

    engine.poll_pty();
    assert!(
        engine.pty_hold.is_some() || engine.pty_rx.try_recv().is_ok(),
        "budgeted poll must leave PTY work for a later tick"
    );
}

#[test]
fn poll_pty_sets_drain_pending_without_requiring_mid_tick_wake() {
    let mut engine = Engine::new();
    let id = engine.active_terminal().id.clone();
    {
        let term = engine.active_terminal_mut();
        term.ingest.ensure_live_screen(&mut term.buffer);
    }
    let chunk = vec![b'a'; 4096];
    let over = (PTY_INGEST_BYTES_PER_TICK / chunk.len()) + 8;
    for _ in 0..over {
        engine
            .pty_tx
            .try_send(PtyEvent::Bytes {
                id: id.clone(),
                data: chunk.clone(),
            })
            .expect("queue has room");
    }
    engine.poll_pty();
    assert!(
        engine.pty_work_pending(),
        "budget overrun must leave pty_work_pending for the host timer/retick"
    );
    assert!(engine.take_pty_drain_pending());
}

#[test]
fn ring_trim_anchors_scroll_when_follow_off() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetSettings {
            max_scrollback_lines: 1_000,
        })
        .expect("settings");
    {
        let term = engine.active_terminal_mut();
        term.ingest.ensure_live_screen(&mut term.buffer);
    }
    let id = engine.active_terminal().id.clone();

    let mut blob = Vec::new();
    for i in 0..1_200 {
        blob.extend_from_slice(format!("LINE-{i:04} {}\r\n", "x".repeat(40)).as_bytes());
    }
    for chunk in blob.chunks(4096) {
        engine
            .pty_tx
            .try_send(PtyEvent::Bytes {
                id: id.clone(),
                data: chunk.to_vec(),
            })
            .ok();
        engine.poll_pty();
        engine.tick();
    }
    for _ in 0..80 {
        engine.poll_pty();
        engine.tick();
        if engine.pty_hold.is_none() {
            break;
        }
    }

    engine
        .send_command(Command::SetFollow { follow: false })
        .expect("follow off");
    let _ = engine.send_command(Command::SetWrapLines { wrap: false });
    let max = engine.max_scroll_offset_for_test();
    // Lower half so ~120 trimmed head lines cannot evict the marker.
    let mid = (max * 0.55).max(engine.viewport_row_stride_for_test() * 10.0);
    engine
        .send_command(Command::Scroll { offset: mid })
        .expect("scroll");
    engine.tick();
    let scroll_before = engine.scroll_offset_y_for_test();
    assert!(scroll_before > 1.0, "precondition: scrolled up ({scroll_before})");

    let stride = engine.viewport_row_stride_for_test();
    let first_row = (scroll_before / stride).floor() as usize;
    let marker = {
        let view = engine.active_terminal().active_view();
        view.flat_lines
            .get(first_row)
            .map(|l| l.raw.clone())
            .expect("flat line at viewport top")
    };

    let more = format!("NEWER {}\r\n", "y".repeat(40)).into_bytes().repeat(120);
    for chunk in more.chunks(4096) {
        engine
            .pty_tx
            .try_send(PtyEvent::Bytes {
                id: id.clone(),
                data: chunk.to_vec(),
            })
            .ok();
        engine.poll_pty();
        engine.tick();
    }

    let scroll_after = engine.scroll_offset_y_for_test();
    assert!(
        scroll_after < scroll_before - 1.0,
        "scroll must shrink on trim when Follow off (before={scroll_before} after={scroll_after})"
    );

    let marker_idx = {
        let view = engine.active_terminal().active_view();
        view.flat_lines
            .iter()
            .position(|l| l.raw == marker)
            .expect("anchored marker line must still exist in scrollback")
    };
    let expected_top = marker_idx as f32 * stride;
    assert!(
        (scroll_after - expected_top).abs() < stride * 1.5,
        "scroll should keep marker near viewport top (scroll={scroll_after} expected≈{expected_top} idx={marker_idx})"
    );
}

#[test]
fn cat_big_log_poll_ticks_stay_bounded() {
    let path = Path::new("/home/dima/big.log");
    if !path.exists() {
        return;
    }
    let mut engine = Engine::new();
    engine.start_interactive_shell();
    // Let the shell start.
    for _ in 0..50 {
        engine.tick();
        std::thread::sleep(Duration::from_millis(20));
        if engine.active_terminal().running {
            break;
        }
    }
    assert!(
        engine.active_terminal().running,
        "interactive shell should be running"
    );
    // Extra settle so the prompt is ready.
    for _ in 0..10 {
        engine.tick();
        std::thread::sleep(Duration::from_millis(30));
    }

    let before = engine.active_terminal().buffer.records_len();
    let cmd = format!("cat {}\n", path.display());
    engine.handle_key(cmd.as_bytes());

    let deadline = Instant::now() + Duration::from_secs(90);
    let mut max_tick = Duration::ZERO;
    let mut ticks = 0u32;
    let mut saw_growth = false;
    let mut quiet_streak = 0u32;
    while Instant::now() < deadline {
        let t0 = Instant::now();
        engine.tick();
        let dt = t0.elapsed();
        max_tick = max_tick.max(dt);
        ticks += 1;
        assert!(
            dt < Duration::from_millis(750),
            "single tick took {dt:?} (budgeted poll must keep UI responsive)"
        );
        let now_len = engine.active_terminal().buffer.records_len();
        if now_len > before + 100 {
            saw_growth = true;
        }
        let pending = engine.pty_hold.is_some();
        if saw_growth && !pending {
            quiet_streak += 1;
            if quiet_streak >= 20 {
                break;
            }
        } else {
            quiet_streak = 0;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        saw_growth,
        "expected scrollback growth from cat; ticks={ticks} max_tick={max_tick:?}"
    );
    assert!(
        ticks > 20,
        "expected many budgeted ticks draining cat; ticks={ticks} max_tick={max_tick:?}"
    );
    eprintln!("cat_big_log: ticks={ticks} max_tick={max_tick:?}");
}
