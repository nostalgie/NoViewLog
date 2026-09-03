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
    assert!(
        engine.defer_pty_reader_wake(),
        "reader wake must be deferred until the paint interval so the host does not busy-drain"
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

#[test]
fn flood_paint_dirty_is_cadenced_and_follow_stays_snapped() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetFollow { follow: true })
        .expect("follow");
    {
        let term = engine.active_terminal_mut();
        term.running = true;
        term.ingest.ensure_live_screen(&mut term.buffer);
    }
    let id = engine.active_terminal().id.clone();
    let chunk = vec![b'x'; 4096];

    let over = (PTY_INGEST_BYTES_PER_TICK / chunk.len()) + 2;
    for _ in 0..over {
        engine
            .pty_tx
            .try_send(PtyEvent::Bytes {
                id: id.clone(),
                data: chunk.clone(),
            })
            .expect("queue");
    }
    engine.poll_pty();
    assert!(engine.pty_work_pending());
    assert!(engine.needs_render(), "first flood ingest must dirty");
    let max = engine.max_scroll_offset_for_test();
    let scroll = engine.scroll_offset_y_for_test();
    assert!(
        (scroll - max).abs() < 1.5 || max < 1.0,
        "Follow snap on ingest (scroll={scroll} max={max})"
    );
    let mut buf = vec![0u8; 800 * 400 * 4];
    engine.render(800, 400, &mut buf).expect("render");

    // Cadence helper (independent of slow VTE): just-painted + more_pending must not dirty.
    engine.note_viewport_painted();
    engine.viewport_dirty = false;
    engine.mark_viewport_dirty_after_pty_ingest(true);
    assert!(
        !engine.needs_render(),
        "within paint interval, flood must not dirty"
    );

    // Follow still snaps on a real poll even when paint is deferred.
    engine.note_viewport_painted();
    engine.viewport_dirty = false;
    for _ in 0..over {
        let _ = engine.pty_tx.try_send(PtyEvent::Bytes {
            id: id.clone(),
            data: chunk.clone(),
        });
    }
    // Force "paint was just now" so even a slow poll stays within the cadence window
    // for the dirty decision at the end of poll_pty.
    engine.note_viewport_painted();
    engine.poll_pty();
    // Re-stamp after poll's VTE cost so the assertion targets deferred dirty, not wall clock.
    // Snap must already have happened during poll.
    let max = engine.max_scroll_offset_for_test();
    let scroll = engine.scroll_offset_y_for_test();
    assert!(
        (scroll - max).abs() < 1.5 || max < 1.0,
        "Follow snap when paint may be deferred (scroll={scroll} max={max})"
    );

    std::thread::sleep(Duration::from_millis(40));
    engine.viewport_dirty = false;
    engine.mark_viewport_dirty_after_pty_ingest(true);
    assert!(
        engine.needs_render(),
        "after paint interval, flood ingest must dirty again"
    );

    // Catch-up / echo: more_pending=false always dirties.
    engine.note_viewport_painted();
    engine.viewport_dirty = false;
    engine.mark_viewport_dirty_after_pty_ingest(false);
    assert!(
        engine.needs_render(),
        "idle echo / catch-up must dirty immediately"
    );
}

#[test]
fn follow_scroll_does_not_jump_across_tick_rebuild() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetFollow { follow: true })
        .expect("follow");
    {
        let term = engine.active_terminal_mut();
        term.ingest.ensure_live_screen(&mut term.buffer);
    }
    engine.tick();
    let id = engine.active_terminal().id.clone();
    engine.active_terminal_mut().running = true;
    let chunk = vec![b'x'; 4096];
    let over = (PTY_INGEST_BYTES_PER_TICK / chunk.len()) + 2;
    for _ in 0..over {
        let _ = engine.pty_tx.try_send(PtyEvent::Bytes {
            id: id.clone(),
            data: chunk.clone(),
        });
    }
    engine.poll_pty();
    let max_after_poll = engine.max_scroll_offset_for_test();
    let scroll_after_poll = engine.scroll_offset_y_for_test();
    engine.tick();
    let max_after_tick = engine.max_scroll_offset_for_test();
    let scroll_after_tick = engine.scroll_offset_y_for_test();
    assert!(
        (scroll_after_poll - max_after_poll).abs() < 2.0 || max_after_poll < 1.0,
        "Follow snap after poll (scroll={scroll_after_poll} max={max_after_poll})"
    );
    assert!(
        (max_after_tick - max_after_poll).abs() < engine.viewport_row_stride_for_test() * 2.0
            || max_after_poll < 1.0,
        "tick rebuild must not drop overlay height (max poll={max_after_poll} tick={max_after_tick})"
    );
    assert!(
        (scroll_after_tick - max_after_tick).abs() < 2.0 || max_after_tick < 1.0,
        "Follow must stay snapped after tick (scroll={scroll_after_tick} max={max_after_tick})"
    );
}

#[test]
fn follow_flood_does_not_patch_overlay_into_logview() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetFollow { follow: true })
        .expect("follow");
    {
        let term = engine.active_terminal_mut();
        term.running = true;
        term.ingest.ensure_live_screen(&mut term.buffer);
    }
    let before_lines = engine.active_view().flat_lines.len();
    let before_overlay = engine.active_view().overlay_len();
    let id = engine.active_terminal().id.clone();
    let line = format!("{}\r\n", "x".repeat(60));
    let blob = line.repeat(400);
    for chunk in blob.as_bytes().chunks(4096) {
        let _ = engine.pty_tx.try_send(PtyEvent::Bytes {
            id: id.clone(),
            data: chunk.to_vec(),
        });
    }
    engine.poll_pty();
    assert_eq!(
        engine.active_view().overlay_len(),
        before_overlay,
        "Follow must not replace overlay on the Terminal tab LogView"
    );
    assert_eq!(
        engine.active_view().flat_lines.len(),
        before_lines,
        "Follow must not grow Terminal tab flat_lines under flood"
    );
    assert!(
        engine.active_terminal().buffer.records_len() > 10,
        "scrolled-off rows must still become Records"
    );
    let mut buf = vec![0u8; 800 * 400 * 4];
    engine.render(800, 400, &mut buf).expect("live grid render");
}

#[test]
fn echo_does_not_defer_pty_reader_wake() {
    let mut engine = Engine::new();
    let id = engine.active_terminal().id.clone();
    engine
        .pty_tx
        .try_send(PtyEvent::Bytes {
            id,
            data: b"x".to_vec(),
        })
        .expect("echo");
    engine.poll_pty();
    assert!(
        !engine.pty_work_pending(),
        "single echo must drain in one poll"
    );
    assert!(
        !engine.defer_pty_reader_wake(),
        "echo must still allow an immediate host wake"
    );
}

#[test]
fn flood_wake_defer_clears_after_paint_interval() {
    let mut engine = Engine::new();
    let id = engine.active_terminal().id.clone();
    let chunk = vec![b'a'; 4096];
    let over = (PTY_INGEST_BYTES_PER_TICK / chunk.len()) + 8;
    for _ in 0..over {
        let _ = engine.pty_tx.try_send(PtyEvent::Bytes {
            id: id.clone(),
            data: chunk.clone(),
        });
    }
    engine.poll_pty();
    assert!(engine.defer_pty_reader_wake());
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        !engine.defer_pty_reader_wake(),
        "after the paint interval the host timer may poll again"
    );
}

#[test]
fn follow_wrap_live_grid_render_does_not_panic() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetFollow { follow: true })
        .expect("follow");
    engine
        .send_command(Command::SetWrapLines { wrap: true })
        .expect("wrap");
    {
        let term = engine.active_terminal_mut();
        term.running = true;
        term.ingest.ensure_live_screen(&mut term.buffer);
        let long = format!("{}\r\n", "https://example.com/path/").repeat(30);
        term.ingest
            .feed(long.as_bytes(), &mut term.buffer, &mut term.parser);
    }
    let mut buf = vec![0u8; 800 * 400 * 4];
    engine
        .render(800, 400, &mut buf)
        .expect("Follow+WRAP live grid");
    assert!(engine.wrap_lines_for_test());
}

#[test]
fn follow_live_line_counter_grows_past_scrollback_cap() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::SetSettings {
            max_scrollback_lines: 200,
        })
        .expect("settings");
    engine
        .send_command(Command::SetFollow { follow: true })
        .expect("follow");
    {
        let term = engine.active_terminal_mut();
        term.running = true;
        term.ingest.ensure_live_screen(&mut term.buffer);
        let mut blob = String::new();
        for i in 0..800 {
            blob.push_str(&format!("line-{i}\n"));
        }
        term.ingest
            .feed(blob.as_bytes(), &mut term.buffer, &mut term.parser);
    }
    assert!(
        engine.active_terminal().buffer.dropped_count() > 0,
        "ring must have trimmed past the 200-line cap"
    );
    let (cur, total) = engine.viewport_line_position_for_test();
    assert_eq!(cur, total);
    assert!(
        total > 200,
        "Follow status must grow past max_scrollback (got {total}), not stick at the ring size"
    );
    // Scrollbar range is ring + screen (not ever-seen), so the thumb stays small.
    let max = engine.max_scroll_offset_for_test();
    let stride = engine.viewport_row_stride_for_test();
    let ever = total as f32 * stride;
    assert!(
        max < ever * 0.5,
        "live Follow max_scroll must not use ever-seen height (max={max} ever≈{ever})"
    );
    assert!(
        max > 50.0 * stride,
        "live Follow max_scroll should include the retained ring, not screen-only (max={max})"
    );
}

#[test]
fn selection_materializes_live_grid_under_follow() {
    let mut engine = Engine::new();
    engine
        .send_command(Command::Resize {
            width: 800,
            height: 400,
        })
        .expect("resize");
    engine.mark_running_for_test();
    engine.ensure_live_screen_for_test();
    {
        let term = engine.active_terminal_mut();
        term.ingest.feed(
            b"hello selection marker line here\r\n",
            &mut term.buffer,
            &mut term.parser,
        );
    }
    assert!(engine.auto_follow_for_test());
    assert_eq!(
        engine.view_flat_line_count_for_test(0),
        Some(0),
        "live grid must not patch flat_lines while Follow paints the VT screen"
    );

    engine
        .send_command(Command::SelectionAt {
            x: 80.0,
            y: 20.0,
            extend: false,
            click_count: 1,
        })
        .expect("selection down");
    assert!(!engine.auto_follow_for_test());
    assert!(
        engine.view_flat_line_count_for_test(0).unwrap_or(0) > 0,
        "click must materialize live screen into flat_lines"
    );

    let stride = engine.viewport_row_stride_for_test();
    let scroll = engine.scroll_offset_y_for_test();
    let idx = engine
        .flat_line_texts_for_test()
        .iter()
        .position(|l| l.contains("selection marker"))
        .expect("materialized line with marker");
    let y = idx as f32 * stride - scroll + stride * 0.5;

    engine
        .send_command(Command::SelectionAt {
            x: 120.0,
            y,
            extend: false,
            click_count: 2,
        })
        .expect("word select");
    let text = engine.selection_text_for_test().unwrap_or_default();
    assert!(
        text.contains("selection"),
        "expected word selection, got {text:?}"
    );
}

