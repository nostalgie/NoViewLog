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
    view.rebuild(&buffer);
    assert_eq!(view.flat_lines_record_cursor, buffer.records().len());
    let stable_prefix = view.flat_lines.len() - ingest.volatile_count();

    for b in b"hello world!!!" {
        let old_vol = ingest.volatile_count();
        let old_total = buffer.records().len();
        ingest.feed(&[*b], &mut buffer, &mut parser);
        let new_vol = ingest.volatile_count();
        let new_total = buffer.records().len();
        assert!(
            view.try_patch_volatile_tail(&buffer, old_vol, old_total, new_vol, new_total),
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
        view.rebuild(&buffer);
        assert_eq!(
            view.flat_lines.len(),
            before_len,
            "rebuild after successful patch must be a no-op on length"
        );
    }
}
