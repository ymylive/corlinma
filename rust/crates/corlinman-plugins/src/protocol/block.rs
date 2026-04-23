//! Structured-block tool-call parser.
//!
//! Grammar:
//! ```text
//! <<<[TOOL_REQUEST]>>>
//! <key>:「始」<value>「末」,
//! <key>:「始」<value>「末」
//! <<<[END_TOOL_REQUEST]>>>
//! ```
//!
//! - `<key>` matches `[A-Za-z_][A-Za-z0-9_]*`.
//! - `<value>` is arbitrary (multi-line, CJK, JSON) text between `「始」`
//!   and `「末」`.
//! - A trailing comma after the last entry is tolerated.
//! - Any whitespace between entries is skipped.
//! - Multiple envelopes may appear in one source string.
//!
//! All argument values are captured as raw strings; type coercion against
//! the tool's JSON-schema happens in [`coerce_args`].

use std::collections::HashMap;

use serde_json::Value;

/// Envelope open marker.
const OPEN: &str = "<<<[TOOL_REQUEST]>>>";
/// Envelope close marker.
const CLOSE: &str = "<<<[END_TOOL_REQUEST]>>>";
/// Value-start marker (CJK 「始」, 9 bytes UTF-8).
const VAL_START: &str = "\u{300C}\u{59CB}\u{300D}"; // 「始」
/// Value-end marker (CJK 「末」, 9 bytes UTF-8).
const VAL_END: &str = "\u{300C}\u{672B}\u{300D}"; // 「末」

/// A parsed tool call extracted from a single `<<<[TOOL_REQUEST]>>>`
/// envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockToolCall {
    /// Name the model chose for the tool (value of the `tool_name` key).
    pub tool_name: String,
    /// Arguments after schema-driven coercion.
    pub args: HashMap<String, Value>,
    /// Arguments as they appeared verbatim in the source (pre-coercion).
    pub raw_args: HashMap<String, String>,
    /// Byte offsets `(start, end_exclusive)` of the envelope in the
    /// source string. `start` points at `<` of `<<<[TOOL_REQUEST]>>>`,
    /// `end` points just past `>` of `<<<[END_TOOL_REQUEST]>>>`.
    pub span: (usize, usize),
}

/// Errors produced by [`parse_all`] and [`coerce_args`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockParseError {
    /// Found an opening `<<<[TOOL_REQUEST]>>>` but no matching close.
    UnterminatedEnvelope { start: usize },
    /// Found `key:「始」` but no `「末」` before end of envelope.
    UnterminatedArgument { key: String, start: usize },
    /// The envelope parsed, but there was no `tool_name` key.
    MissingToolName,
    /// A raw string could not be coerced to the JSON-schema type.
    Coercion { key: String, reason: String },
}

impl std::fmt::Display for BlockParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedEnvelope { start } => {
                write!(f, "unterminated TOOL_REQUEST envelope at byte {start}")
            }
            Self::UnterminatedArgument { key, start } => {
                write!(
                    f,
                    "unterminated argument value for key `{key}` at byte {start}"
                )
            }
            Self::MissingToolName => write!(f, "tool block has no `tool_name` key"),
            Self::Coercion { key, reason } => {
                write!(f, "failed to coerce arg `{key}`: {reason}")
            }
        }
    }
}

impl std::error::Error for BlockParseError {}

/// Parse every `<<<[TOOL_REQUEST]>>>` envelope in `source`.
///
/// Returns one `Result` per envelope, in source order. An envelope that
/// starts but never closes produces an `Err(UnterminatedEnvelope)`; the
/// scanner then stops (we cannot reliably find further envelopes
/// inside a malformed one).
pub fn parse_all(source: &str) -> Vec<Result<BlockToolCall, BlockParseError>> {
    let span = tracing::debug_span!(
        "block_parse",
        envelope_count = tracing::field::Empty,
        error_count = tracing::field::Empty,
    );
    let _enter = span.enter();

    let mut out = Vec::new();
    let mut cursor: usize = 0;
    let bytes = source.as_bytes();

    while cursor < bytes.len() {
        // Find the next opener.
        let rel = match source[cursor..].find(OPEN) {
            Some(r) => r,
            None => break,
        };
        let open_start = cursor + rel;
        let body_start = open_start + OPEN.len();

        // Find the matching close.
        let close_rel = match source[body_start..].find(CLOSE) {
            Some(r) => r,
            None => {
                out.push(Err(BlockParseError::UnterminatedEnvelope {
                    start: open_start,
                }));
                return out;
            }
        };
        let body_end = body_start + close_rel;
        let envelope_end = body_end + CLOSE.len();

        let body = &source[body_start..body_end];
        match parse_body(body, body_start) {
            Ok((tool_name, raw)) => out.push(Ok(BlockToolCall {
                tool_name,
                args: raw
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect(),
                raw_args: raw,
                span: (open_start, envelope_end),
            })),
            Err(e) => out.push(Err(e)),
        }

        cursor = envelope_end;
    }

    let envelope_count = out.len() as u64;
    let error_count = out.iter().filter(|r| r.is_err()).count() as u64;
    span.record("envelope_count", envelope_count);
    span.record("error_count", error_count);

    out
}

/// Parse a single envelope body (everything between `<<<[TOOL_REQUEST]>>>`
/// and `<<<[END_TOOL_REQUEST]>>>`). `body_offset` is the byte offset of
/// `body` within the original source, used for error reporting.
fn parse_body(
    body: &str,
    body_offset: usize,
) -> Result<(String, HashMap<String, String>), BlockParseError> {
    let mut raw: HashMap<String, String> = HashMap::new();
    let mut tool_name: Option<String> = None;

    // Work in byte indices relative to `body`.
    let bytes = body.as_bytes();
    let mut i: usize = 0;

    loop {
        // Skip whitespace (ASCII; multibyte whitespace is not expected
        // between structural tokens and would be a grammar mistake).
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Skip optional separator comma.
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
            continue;
        }
        if i >= bytes.len() {
            break;
        }

        // Read key: [A-Za-z_][A-Za-z0-9_]*
        let key_start = i;
        if !is_ident_start(bytes[i]) {
            // Not a key character — bail out of this envelope silently.
            // This keeps us forgiving about trailing garbage. We still
            // require at least `tool_name` to have been parsed.
            break;
        }
        i += 1;
        while i < bytes.len() && is_ident_cont(bytes[i]) {
            i += 1;
        }
        let key = &body[key_start..i];

        // Skip whitespace before colon.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Expect ':'.
        if i >= bytes.len() || bytes[i] != b':' {
            break;
        }
        i += 1;
        // Skip whitespace before VAL_START.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Expect VAL_START.
        if !body[i..].starts_with(VAL_START) {
            break;
        }
        let value_start = i + VAL_START.len();

        // Locate VAL_END.
        let end_rel = match body[value_start..].find(VAL_END) {
            Some(r) => r,
            None => {
                return Err(BlockParseError::UnterminatedArgument {
                    key: key.to_string(),
                    start: body_offset + key_start,
                });
            }
        };
        let value = &body[value_start..value_start + end_rel];

        if key == "tool_name" {
            tool_name = Some(value.to_string());
        } else {
            raw.insert(key.to_string(), value.to_string());
        }

        i = value_start + end_rel + VAL_END.len();
    }

    match tool_name {
        Some(name) => Ok((name, raw)),
        None => Err(BlockParseError::MissingToolName),
    }
}

#[inline]
fn is_ident_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

#[inline]
fn is_ident_cont(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Coerce the raw string args against a JSON schema's `properties` map.
///
/// - `"string"` → `Value::String` (unchanged).
/// - `"integer"` → parsed as `i64`.
/// - `"number"` → parsed as `f64`; NaN rejected.
/// - `"boolean"` → accepts `true/false/1/0/yes/no`, case-insensitive.
/// - `"array"` / `"object"` → `serde_json::from_str`.
/// - Unknown or missing schema entry → pass through as `Value::String`.
pub fn coerce_args(
    raw: &HashMap<String, String>,
    schema: &Value,
) -> Result<HashMap<String, Value>, BlockParseError> {
    let mut out = HashMap::with_capacity(raw.len());
    let properties = schema.get("properties").and_then(Value::as_object);

    for (key, raw_value) in raw {
        let declared_type = properties
            .and_then(|p| p.get(key))
            .and_then(|p| p.get("type"))
            .and_then(Value::as_str);

        let coerced = match declared_type {
            Some("string") | None => Value::String(raw_value.clone()),
            Some("integer") => raw_value
                .trim()
                .parse::<i64>()
                .map(|n| Value::Number(n.into()))
                .map_err(|e| BlockParseError::Coercion {
                    key: key.clone(),
                    reason: format!("expected integer: {e}"),
                })?,
            Some("number") => {
                let n: f64 = raw_value
                    .trim()
                    .parse()
                    .map_err(|e: std::num::ParseFloatError| BlockParseError::Coercion {
                        key: key.clone(),
                        reason: format!("expected number: {e}"),
                    })?;
                if n.is_nan() {
                    return Err(BlockParseError::Coercion {
                        key: key.clone(),
                        reason: "NaN is not a valid number".into(),
                    });
                }
                match serde_json::Number::from_f64(n) {
                    Some(num) => Value::Number(num),
                    None => {
                        return Err(BlockParseError::Coercion {
                            key: key.clone(),
                            reason: "number not representable as JSON".into(),
                        });
                    }
                }
            }
            Some("boolean") => {
                let lowered = raw_value.trim().to_ascii_lowercase();
                match lowered.as_str() {
                    "true" | "1" | "yes" => Value::Bool(true),
                    "false" | "0" | "no" => Value::Bool(false),
                    other => {
                        return Err(BlockParseError::Coercion {
                            key: key.clone(),
                            reason: format!("expected boolean, got `{other}`"),
                        });
                    }
                }
            }
            Some("array") | Some("object") => serde_json::from_str::<Value>(raw_value.trim())
                .map_err(|e| BlockParseError::Coercion {
                    key: key.clone(),
                    reason: format!("invalid JSON for {}: {e}", declared_type.unwrap()),
                })?,
            // Unknown type label → lenient pass-through.
            Some(_) => Value::String(raw_value.clone()),
        };

        out.insert(key.clone(), coerced);
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(result: Result<BlockToolCall, BlockParseError>) -> BlockToolCall {
        result.expect("expected successful parse")
    }

    #[test]
    fn single_block_with_two_args_parses() {
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」shell「末」,
command:「始」ls -la「末」
<<<[END_TOOL_REQUEST]>>>";
        let mut calls = parse_all(src);
        assert_eq!(calls.len(), 1);
        let call = ok(calls.remove(0));
        assert_eq!(call.tool_name, "shell");
        assert_eq!(
            call.raw_args.get("command").map(String::as_str),
            Some("ls -la")
        );
        assert_eq!(call.span.0, 0);
        assert_eq!(call.span.1, src.len());
    }

    #[test]
    fn multiple_blocks_in_one_string_all_parsed() {
        let src = "noise
<<<[TOOL_REQUEST]>>>
tool_name:「始」a「末」
<<<[END_TOOL_REQUEST]>>>
middle junk
<<<[TOOL_REQUEST]>>>
tool_name:「始」b「末」,
x:「始」1「末」
<<<[END_TOOL_REQUEST]>>>
tail";
        let calls = parse_all(src);
        assert_eq!(calls.len(), 2);
        assert_eq!(ok(calls[0].clone()).tool_name, "a");
        let second = ok(calls[1].clone());
        assert_eq!(second.tool_name, "b");
        assert_eq!(second.raw_args.get("x").map(String::as_str), Some("1"));
    }

    #[test]
    fn unterminated_envelope_returns_err_with_start_offset() {
        let src = "prefix <<<[TOOL_REQUEST]>>>\ntool_name:「始」oops「末」";
        let calls = parse_all(src);
        assert_eq!(calls.len(), 1);
        match &calls[0] {
            Err(BlockParseError::UnterminatedEnvelope { start }) => {
                assert_eq!(*start, "prefix ".len());
            }
            other => panic!("expected UnterminatedEnvelope, got {other:?}"),
        }
    }

    #[test]
    fn unterminated_argument_returns_err() {
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」x「末」,
bad_key:「始」no end marker here
<<<[END_TOOL_REQUEST]>>>";
        let calls = parse_all(src);
        assert_eq!(calls.len(), 1);
        match &calls[0] {
            Err(BlockParseError::UnterminatedArgument { key, .. }) => {
                assert_eq!(key, "bad_key");
            }
            other => panic!("expected UnterminatedArgument, got {other:?}"),
        }
    }

    #[test]
    fn missing_tool_name_is_err() {
        let src = "\
<<<[TOOL_REQUEST]>>>
only_arg:「始」v「末」
<<<[END_TOOL_REQUEST]>>>";
        let calls = parse_all(src);
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0], Err(BlockParseError::MissingToolName)));
    }

    #[test]
    fn multiline_argument_value_preserved_verbatim() {
        let value = "line 1\nline 2\n  indented\n{\"k\":「nested CJK」}";
        let src = format!(
            "<<<[TOOL_REQUEST]>>>\ntool_name:「始」t「末」,\nbody:「始」{value}「末」\n<<<[END_TOOL_REQUEST]>>>"
        );
        let mut calls = parse_all(&src);
        let call = ok(calls.remove(0));
        assert_eq!(call.raw_args.get("body").map(String::as_str), Some(value));
    }

    #[test]
    fn trailing_comma_tolerated() {
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」t「末」,
a:「始」1「末」,
<<<[END_TOOL_REQUEST]>>>";
        let call = ok(parse_all(src).remove(0));
        assert_eq!(call.raw_args.get("a").map(String::as_str), Some("1"));
    }

    #[test]
    fn whitespace_between_args_tolerated() {
        let src = "<<<[TOOL_REQUEST]>>>\n\n  tool_name :「始」t「末」\n\n\t a  :「始」1「末」   ,\n  \n<<<[END_TOOL_REQUEST]>>>";
        let call = ok(parse_all(src).remove(0));
        assert_eq!(call.tool_name, "t");
        assert_eq!(call.raw_args.get("a").map(String::as_str), Some("1"));
    }

    #[test]
    fn coerce_integer_boolean_number() {
        let schema = json!({
            "properties": {
                "count": { "type": "integer" },
                "flag":  { "type": "boolean" },
                "ratio": { "type": "number"  }
            }
        });
        let mut raw = HashMap::new();
        raw.insert("count".into(), "42".into());
        raw.insert("flag".into(), "YES".into());
        raw.insert("ratio".into(), "2.5".into());

        let out = coerce_args(&raw, &schema).unwrap();
        assert_eq!(out["count"], Value::Number(42i64.into()));
        assert_eq!(out["flag"], Value::Bool(true));
        assert_eq!(out["ratio"].as_f64().unwrap(), 2.5);
    }

    #[test]
    fn coerce_invalid_integer_returns_coercion_error() {
        let schema = json!({ "properties": { "n": { "type": "integer" } } });
        let mut raw = HashMap::new();
        raw.insert("n".into(), "not-a-number".into());
        match coerce_args(&raw, &schema) {
            Err(BlockParseError::Coercion { key, .. }) => assert_eq!(key, "n"),
            other => panic!("expected Coercion, got {other:?}"),
        }
    }

    #[test]
    fn coerce_object_arg_parses_json_object() {
        let schema = json!({ "properties": { "payload": { "type": "object" } } });
        let mut raw = HashMap::new();
        raw.insert("payload".into(), r#"{"a":1,"b":"two"}"#.into());
        let out = coerce_args(&raw, &schema).unwrap();
        assert_eq!(out["payload"], json!({"a": 1, "b": "two"}));
    }

    #[test]
    fn coerce_unknown_schema_key_passes_through_as_string() {
        let schema = json!({ "properties": { "known": { "type": "integer" } } });
        let mut raw = HashMap::new();
        raw.insert("mystery".into(), "whatever".into());
        let out = coerce_args(&raw, &schema).unwrap();
        assert_eq!(out["mystery"], Value::String("whatever".into()));
    }

    #[test]
    fn utf8_offsets_are_boundary_safe() {
        // An argument full of multi-byte characters; check that span
        // and value extraction land on UTF-8 boundaries.
        let src = "\
前言
<<<[TOOL_REQUEST]>>>
tool_name:「始」中文工具「末」,
note:「始」你好，世界🌏「末」
<<<[END_TOOL_REQUEST]>>>
尾声";
        let call = ok(parse_all(src).remove(0));
        assert_eq!(call.tool_name, "中文工具");
        assert_eq!(
            call.raw_args.get("note").map(String::as_str),
            Some("你好，世界🌏")
        );
        // Span is on UTF-8 boundaries because Rust slicing would have
        // panicked otherwise — re-slice to double-check.
        let _slice = &src[call.span.0..call.span.1];
    }
}
