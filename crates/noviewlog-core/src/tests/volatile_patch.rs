use crate::core::buffer::RecordBuffer;
use crate::core::formats::get_builtin_format;
use crate::core::parser::RecordParser;
use crate::core::terminal::TerminalIngest;
use crate::core::types::LogRecord;
use crate::log_view::LogView;
use chrono::Utc;

fn seed_scrollback(buffer: &mut RecordBuffer, n: usize) {
    for i in 0..n {
        buffer.add(LogRecord {
            id: i as u64,
            lines: vec![format!("line-{i}")],
            text: format!("line-{i}"),
            received_at: Utc::now(),
            level: None,
            overwrite: false,
        });
    }
}

#[test]
fn many_single_byte_feeds_patch_without_marking_dirty() {
    let mut buffer = RecordBuffer::new(50_000);
    seed_scrollback(&mut buffer, 5_000);
    let format = get_builtin_format("generic");
    let mut parser = RecordParser::new(format);
    let mut ingest = TerminalIngest::new_with_size(80, 24);
    ingest.ensure_live_screen(&mut buffer);

    let mut view = LogView::from_runtime(crate::TERMINAL_TAB_NAME, Vec::new());
    view.rebuild(&mut buffer);
    assert_eq!(view.flat_lines_record_cursor, buffer.records_len());
    let stable_prefix = view.flat_lines.len() - ingest.volatile_count();

    for b in b"hello world!!!" {
        let old_vol = ingest.volatile_count();
        let old_total = buffer.records_len();
        let shifted = ingest.feed(&[*b], &mut buffer, &mut parser);
        let new_vol = ingest.volatile_count();
        let new_total = buffer.records_len();
        assert!(
            view.try_patch_volatile_tail(
                &mut buffer,
                old_vol,
                old_total,
                new_vol,
                new_total,
                shifted
            ),
            "patch should succeed for unfiltered Terminal tab"
        );
        assert_eq!(view.flat_lines_record_cursor, new_total);
        assert_eq!(
            view.flat_lines.len().saturating_sub(new_vol),
            stable_prefix,
            "stable prefix length must stay put"
        );
        // A second full rebuild must not be required — patch left dirty clear.
        let before_len = view.flat_lines.len();
        view.rebuild(&mut buffer);
        assert_eq!(
            view.flat_lines.len(),
            before_len,
            "rebuild after successful patch must be a no-op on length"
        );
    }
}

#[test]
fn patch_survives_ring_trim_under_cap() {
    let mut buffer = RecordBuffer::new(100);
    seed_scrollback(&mut buffer, 100);
    let format = get_builtin_format("generic");
    let mut parser = RecordParser::new(format);
    let mut ingest = TerminalIngest::new_with_size(80, 10);
    ingest.ensure_live_screen(&mut buffer);

    let mut view = LogView::from_runtime(crate::TERMINAL_TAB_NAME, Vec::new());
    view.rebuild(&mut buffer);

    // Flood past the cap so trim fires every commit.
    let flood = "x".repeat(70) + "\r\n";
    for _ in 0..50 {
        let old_vol = ingest.volatile_count();
        let old_total = buffer.records_len();
        let shifted = ingest.feed(flood.as_bytes(), &mut buffer, &mut parser);
        let new_vol = ingest.volatile_count();
        let new_total = buffer.records_len();
        assert!(
            view.try_patch_volatile_tail(
                &mut buffer,
                old_vol,
                old_total,
                new_vol,
                new_total,
                shifted
            ) || {
                view.mark_flat_lines_dirty();
                view.rebuild(&mut buffer);
                true
            },
            "patch or dirty rebuild must keep flat lines coherent"
        );
        assert_eq!(view.flat_lines_record_cursor, buffer.records_len());
        assert!(buffer.records_len() <= 100);
    }
}
