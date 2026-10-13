//! Tests for [`read_stdout_lines_bounded`]: the per-line byte cap is applied on
//! the read, an over-sized line yields exactly one `Error`, and normal framing
//! (LF/CRLF, final unterminated line, U+2028 inside records) is unchanged.

use std::io::Cursor;
use std::sync::mpsc;

use serde_json::json;

use super::jsonl_process::{read_stdout_lines_bounded, JsonlRecord};

/// Small cap so the over-sized path is exercised with tiny inputs.
const CAP: usize = 16;

/// Feed `input` through the bounded reader on a detached channel and collect
/// every record (dropping the sender explicitly so the iterator can end).
fn drain(input: Vec<u8>) -> Vec<JsonlRecord> {
    let (sender, receiver) = mpsc::channel();
    read_stdout_lines_bounded(Cursor::new(input), &sender, CAP);
    drop(sender);
    receiver.iter().collect()
}

#[test]
fn short_valid_lines_still_arrive_as_records_then_exited() {
    let input = b"{\"a\":1}\n{\"b\":2}\n".to_vec();
    let records = drain(input);
    assert_eq!(records.len(), 3);
    match &records[0] {
        JsonlRecord::Record(value) => assert_eq!(value, &json!({ "a": 1 })),
        other => panic!("expected Record, got {other:?}"),
    }
    match &records[1] {
        JsonlRecord::Record(value) => assert_eq!(value, &json!({ "b": 2 })),
        other => panic!("expected Record, got {other:?}"),
    }
    assert!(matches!(records[2], JsonlRecord::Exited));
}

#[test]
fn oversized_line_between_short_lines_yields_one_error_and_keeps_parsing() {
    let short = b"{\"a\":1}".to_vec();
    let mut input = short.clone();
    input.push(b'\n');
    input.extend(std::iter::repeat_n(b'x', 40));
    input.push(b'\n');
    input.extend(&short);
    input.push(b'\n');

    let records = drain(input);
    assert_eq!(records.len(), 4, "got {records:?}");
    match &records[0] {
        JsonlRecord::Record(value) => assert_eq!(value, &json!({ "a": 1 })),
        other => panic!("expected Record, got {other:?}"),
    }
    match &records[1] {
        JsonlRecord::Error(text) => assert!(text.contains("exceeds"), "got {text}"),
        other => panic!("expected Error, got {other:?}"),
    }
    // The line after the discarded one is still parsed.
    match &records[2] {
        JsonlRecord::Record(value) => assert_eq!(value, &json!({ "a": 1 })),
        other => panic!("expected Record, got {other:?}"),
    }
    assert!(matches!(records[3], JsonlRecord::Exited));
}

#[test]
fn line_of_exactly_cap_bytes_is_a_record() {
    // `{"a":"12345678"}` is exactly 16 content bytes.
    let input = b"{\"a\":\"12345678\"}\n".to_vec();
    assert_eq!(input.len() - 1, CAP);

    let records = drain(input);
    assert_eq!(records.len(), 2);
    match &records[0] {
        JsonlRecord::Record(value) => assert_eq!(value, &json!({ "a": "12345678" })),
        other => panic!("expected Record, got {other:?}"),
    }
    assert!(matches!(records[1], JsonlRecord::Exited));
}

#[test]
fn line_of_cap_plus_one_bytes_is_an_error() {
    // `{"a":"123456789"}` is 17 content bytes.
    let input = b"{\"a\":\"123456789\"}\n".to_vec();
    assert_eq!(input.len() - 1, CAP + 1);

    let records = drain(input);
    assert_eq!(records.len(), 2);
    match &records[0] {
        JsonlRecord::Error(text) => assert!(text.contains("exceeds"), "got {text}"),
        other => panic!("expected Error, got {other:?}"),
    }
    assert!(matches!(records[1], JsonlRecord::Exited));
}

#[test]
fn final_oversized_line_without_trailing_lf_yields_one_error_then_exited() {
    let input = vec![b'y'; 40];

    let records = drain(input);
    assert_eq!(records.len(), 2, "got {records:?}");
    match &records[0] {
        JsonlRecord::Error(text) => assert!(text.contains("exceeds"), "got {text}"),
        other => panic!("expected Error, got {other:?}"),
    }
    assert!(matches!(records[1], JsonlRecord::Exited));
}

#[test]
fn two_consecutive_oversized_lines_yield_two_errors() {
    let mut input = vec![b'z'; 30];
    input.push(b'\n');
    input.extend(std::iter::repeat_n(b'w', 30));
    input.push(b'\n');

    let records = drain(input);
    assert_eq!(records.len(), 3, "got {records:?}");
    for record in &records[..2] {
        match record {
            JsonlRecord::Error(text) => assert!(text.contains("exceeds"), "got {text}"),
            other => panic!("expected Error, got {other:?}"),
        }
    }
    assert!(matches!(records[2], JsonlRecord::Exited));
}

#[test]
fn crlf_terminated_short_line_still_parses() {
    let input = b"{\"c\":3}\r\n".to_vec();

    let records = drain(input);
    assert_eq!(records.len(), 2);
    match &records[0] {
        JsonlRecord::Record(value) => assert_eq!(value, &json!({ "c": 3 })),
        other => panic!("expected Record, got {other:?}"),
    }
    assert!(matches!(records[1], JsonlRecord::Exited));
}

#[test]
fn u2028_inside_record_shorter_than_cap_is_one_record() {
    // U+2028 (3 UTF-8 bytes) inside the JSON string; total 13 content bytes < CAP.
    let mut input = b"{\"s\":\"a".to_vec();
    input.extend([0xE2, 0x80, 0xA8]); // U+2028
    input.extend(b"b\"}\n");

    let records = drain(input);
    assert_eq!(records.len(), 2, "got {records:?}");
    match &records[0] {
        JsonlRecord::Record(value) => assert_eq!(value, &json!({ "s": "a\u{2028}b" })),
        other => panic!("expected Record, got {other:?}"),
    }
    assert!(matches!(records[1], JsonlRecord::Exited));
}
