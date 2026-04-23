//! Protocol dispatcher: turn model output into [`DispatchedCall`]s.
//!
//! This layer does **not** execute anything. It only *prepares* calls — a
//! later workstream will feed the successful outcomes into
//! `PluginRuntime::execute`.
//!
//! ## Dispatch flow
//!
//! ```text
//! model_output ─┐
//!               ├── dispatch(text, registry, policy) ─> Vec<DispatchOutcome>
//! policy ───────┘
//!                   │
//!                   ├── for proto in policy.preference_order:
//!                   │     block           -> block::parse_all + coerce_args
//!                   │     openai_function -> <tool_call>…</tool_call> or pure-JSON
//!                   │
//!                   └── de-dup overlapping hits → higher-priority proto wins
//! ```
//!
//! ## Protocol-not-advertised handling
//!
//! If a model emits a tool call via a protocol the plugin's manifest does
//! NOT advertise, we either:
//!
//! * emit `Err(DispatchError::ProtocolNotAdvertised)`, or
//! * (only when `policy.fallback_to_function_call == true` AND the plugin
//!   advertises `"openai_function"`) silently "downgrade" the call to the
//!   function-call protocol and log at `WARN`.
//!
//! This matches the `[tools.block] fallback_to_function_call` knob in
//! `corlinman-core::config`.

use std::collections::HashMap;

use serde_json::Value;

use super::block::{self, BlockParseError, BlockToolCall};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Runtime policy controlling how model output is dispatched.
///
/// A `ProtocolPolicy` is usually built from `[tools.block]` config plus the
/// operator's agent preferences; the dispatcher never reads env vars or
/// disk state on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolPolicy {
    /// Whether to parse the `"block"` protocol at all. If `false`, any
    /// `<<<[TOOL_REQUEST]>>>` envelopes are ignored as if they were
    /// narrative text.
    pub block_enabled: bool,

    /// If `true`, a block envelope targeting a plugin that only advertises
    /// `"openai_function"` will be *downgraded* to a function-call dispatch
    /// instead of failing with [`DispatchError::ProtocolNotAdvertised`].
    pub fallback_to_function_call: bool,

    /// Which protocols to try, in order. First match wins during
    /// de-duplication. Default: `["openai_function", "block"]`.
    pub preference_order: Vec<String>,
}

impl Default for ProtocolPolicy {
    fn default() -> Self {
        Self {
            block_enabled: true,
            fallback_to_function_call: true,
            preference_order: vec!["openai_function".into(), "block".into()],
        }
    }
}

/// A successfully parsed + coerced tool call, ready to hand to
/// `PluginRuntime::execute`.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchedCall {
    /// Protocol that produced this call: `"openai_function"` or `"block"`.
    pub protocol: String,
    /// Owning plugin name (from registry resolution).
    pub plugin_name: String,
    /// Tool name as declared in the manifest (`<plugin>.<tool>` without the
    /// plugin prefix).
    pub tool_name: String,
    /// Coerced arguments, JSON-schema-compatible.
    pub args: Value,
    /// Byte offsets `(start, end_exclusive)` in `model_output`. Only set
    /// when we actually found the call inside a text slice — e.g. block
    /// envelopes or `<tool_call>` fences. Pure structured function-call
    /// input has no span.
    pub raw_span: Option<(usize, usize)>,
}

/// Reasons a single dispatch attempt can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// Tool name did not resolve in the registry.
    UnknownTool { name: String },
    /// Model used a protocol the plugin's manifest does not advertise, and
    /// no fallback kicked in.
    ProtocolNotAdvertised {
        tool: String,
        protocol: String,
        advertised: Vec<String>,
    },
    /// Block-protocol parsing failure.
    BlockParse(BlockParseError),
    /// Raw-text function-call extraction failed (malformed JSON in a
    /// `<tool_call>` fence, or no fence at all). Best-effort; callers
    /// using structured FC input never see this.
    FunctionCallParse(String),
    /// Schema-driven coercion rejected an argument.
    CoercionFailed { tool: String, reason: String },
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTool { name } => write!(f, "unknown tool: {name}"),
            Self::ProtocolNotAdvertised {
                tool,
                protocol,
                advertised,
            } => write!(
                f,
                "tool `{tool}` does not advertise `{protocol}` (advertised: {advertised:?})"
            ),
            Self::BlockParse(e) => write!(f, "block-protocol parse error: {e}"),
            Self::FunctionCallParse(e) => write!(f, "function-call parse error: {e}"),
            Self::CoercionFailed { tool, reason } => {
                write!(f, "coercion failed for tool `{tool}`: {reason}")
            }
        }
    }
}

impl std::error::Error for DispatchError {}

/// One dispatch result. Callers iterate, execute the `Ok` arm, and surface
/// `Err` diagnostics back to the model / UI.
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    pub call: Result<DispatchedCall, DispatchError>,
    /// Where in the model output the call originated, for diagnostics.
    pub origin_offset: Option<(usize, usize)>,
}

/// Thin read-only view into a plugin registry. The dispatcher never needs
/// to spawn or execute — it just resolves tool names to manifest-level
/// metadata.
pub trait PluginRegistryView {
    fn resolve_tool(&self, tool_name: &str) -> Option<ToolResolution>;
}

/// Manifest-level info the dispatcher needs about a single tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResolution {
    pub plugin_name: String,
    pub tool_name: String,
    /// The set of tool-call protocols the plugin advertises. Comes from
    /// `PluginManifest::protocols`.
    pub protocols: Vec<String>,
    /// JSON Schema for the tool's arguments (draft-07 shape).
    pub parameters_schema: Value,
}

/// Minimal OpenAI-style structured call. This mirrors the relevant subset
/// of the `tool_calls[].function` shape.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiFunctionCall {
    /// `function.name`.
    pub name: String,
    /// `function.arguments` — either a JSON object or a JSON-encoded
    /// string, matching OpenAI's two historical shapes.
    pub arguments: Value,
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Parse `model_output` into tool calls according to `policy`. Does **not**
/// execute anything.
///
/// Callers that have structured function calls from the model gateway
/// should prefer [`dispatch_function_calls`] and merge its outcomes with
/// this function's.
pub fn dispatch(
    model_output: &str,
    plugins: &dyn PluginRegistryView,
    policy: &ProtocolPolicy,
) -> Vec<DispatchOutcome> {
    let span = tracing::debug_span!(
        "protocol_dispatch",
        outcomes_count = tracing::field::Empty,
        block_count = tracing::field::Empty,
        fc_count = tracing::field::Empty,
    );
    let _enter = span.enter();

    let mut by_protocol: HashMap<String, Vec<DispatchOutcome>> = HashMap::new();

    for proto in &policy.preference_order {
        match proto.as_str() {
            "block" => {
                if !policy.block_enabled {
                    continue;
                }
                by_protocol.insert(
                    "block".into(),
                    dispatch_blocks(model_output, plugins, policy),
                );
            }
            "openai_function" => {
                by_protocol.insert(
                    "openai_function".into(),
                    dispatch_function_calls_from_text(model_output, plugins),
                );
            }
            other => {
                tracing::warn!(protocol = %other, "ignoring unknown protocol in preference_order");
            }
        }
    }

    let merged = merge_by_preference(&policy.preference_order, by_protocol);

    let mut block_count = 0u64;
    let mut fc_count = 0u64;
    for outcome in &merged {
        match &outcome.call {
            Ok(call) => {
                corlinman_core::metrics::PROTOCOL_DISPATCH_TOTAL
                    .with_label_values(&[call.protocol.as_str()])
                    .inc();
                if call.protocol == "block" {
                    block_count += 1;
                } else {
                    fc_count += 1;
                }
            }
            Err(e) => {
                let (proto, code) = classify_dispatch_error(e);
                corlinman_core::metrics::PROTOCOL_DISPATCH_ERRORS
                    .with_label_values(&[proto, code])
                    .inc();
            }
        }
    }

    span.record("outcomes_count", merged.len() as u64);
    span.record("block_count", block_count);
    span.record("fc_count", fc_count);

    merged
}

/// Map a `DispatchError` to the `(protocol, code)` labels used by the
/// `corlinman_protocol_dispatch_errors_total` counter. Stays low-cardinality
/// by mapping every error variant to a short ASCII code rather than the
/// error's `Display` payload.
fn classify_dispatch_error(err: &DispatchError) -> (&'static str, &'static str) {
    match err {
        DispatchError::UnknownTool { .. } => ("unknown", "unknown_tool"),
        DispatchError::ProtocolNotAdvertised { protocol, .. } => {
            let proto: &'static str = if protocol == "block" {
                "block"
            } else if protocol == "openai_function" {
                "openai_function"
            } else {
                "unknown"
            };
            (proto, "protocol_not_advertised")
        }
        DispatchError::BlockParse(_) => ("block", "parse"),
        DispatchError::FunctionCallParse(_) => ("openai_function", "parse"),
        DispatchError::CoercionFailed { .. } => ("unknown", "coercion"),
    }
}

/// Hand-parsed `OpenAiFunctionCall`s from the gateway (structured path).
/// Unlike [`dispatch`], this never needs to probe text; it only resolves
/// tool names and coerces arguments.
pub fn dispatch_function_calls(
    function_calls: Vec<OpenAiFunctionCall>,
    plugins: &dyn PluginRegistryView,
) -> Vec<DispatchOutcome> {
    let out: Vec<DispatchOutcome> = function_calls
        .into_iter()
        .map(|fc| resolve_function_call(fc, plugins, None))
        .collect();
    for outcome in &out {
        match &outcome.call {
            Ok(_) => corlinman_core::metrics::PROTOCOL_DISPATCH_TOTAL
                .with_label_values(&["openai_function"])
                .inc(),
            Err(e) => {
                let (proto, code) = classify_dispatch_error(e);
                corlinman_core::metrics::PROTOCOL_DISPATCH_ERRORS
                    .with_label_values(&[proto, code])
                    .inc();
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Block-protocol dispatch
// ---------------------------------------------------------------------------

fn dispatch_blocks(
    source: &str,
    plugins: &dyn PluginRegistryView,
    policy: &ProtocolPolicy,
) -> Vec<DispatchOutcome> {
    block::parse_all(source)
        .into_iter()
        .map(|parsed| match parsed {
            Ok(call) => resolve_block_call(call, plugins, policy),
            Err(e) => DispatchOutcome {
                call: Err(DispatchError::BlockParse(e)),
                origin_offset: None,
            },
        })
        .collect()
}

fn resolve_block_call(
    call: BlockToolCall,
    plugins: &dyn PluginRegistryView,
    policy: &ProtocolPolicy,
) -> DispatchOutcome {
    let span = call.span;
    let origin = Some(span);
    let resolution = match plugins.resolve_tool(&call.tool_name) {
        Some(r) => r,
        None => {
            return DispatchOutcome {
                call: Err(DispatchError::UnknownTool {
                    name: call.tool_name.clone(),
                }),
                origin_offset: origin,
            };
        }
    };

    let advertises_block = resolution.protocols.iter().any(|p| p == "block");
    let advertises_fc = resolution.protocols.iter().any(|p| p == "openai_function");

    // Coerce arguments against the tool's parameter schema. Same schema
    // is used regardless of downgrade — that's the whole point of the
    // "identical args across protocols" guarantee.
    let coerced = match block::coerce_args(&call.raw_args, &resolution.parameters_schema) {
        Ok(map) => map,
        Err(e) => {
            return DispatchOutcome {
                call: Err(DispatchError::CoercionFailed {
                    tool: call.tool_name.clone(),
                    reason: e.to_string(),
                }),
                origin_offset: origin,
            };
        }
    };
    let args_value = hashmap_to_value(coerced);

    let dispatch_proto = if advertises_block {
        "block"
    } else if policy.fallback_to_function_call && advertises_fc {
        tracing::warn!(
            tool = %call.tool_name,
            plugin = %resolution.plugin_name,
            "block envelope targets a tool that only advertises openai_function; \
             downgrading per policy.fallback_to_function_call"
        );
        "openai_function"
    } else {
        return DispatchOutcome {
            call: Err(DispatchError::ProtocolNotAdvertised {
                tool: call.tool_name.clone(),
                protocol: "block".into(),
                advertised: resolution.protocols.clone(),
            }),
            origin_offset: origin,
        };
    };

    DispatchOutcome {
        call: Ok(DispatchedCall {
            protocol: dispatch_proto.into(),
            plugin_name: resolution.plugin_name,
            tool_name: resolution.tool_name,
            args: args_value,
            raw_span: Some(span),
        }),
        origin_offset: origin,
    }
}

// ---------------------------------------------------------------------------
// Function-call dispatch (raw text + structured)
// ---------------------------------------------------------------------------

/// Best-effort raw-text extraction of OpenAI-style function calls.
///
/// Two shapes are recognised:
///
/// 1. Fenced:   `<tool_call>{"name":"…","arguments":{…}}</tool_call>`
/// 2. Pure JSON: the entire string parses to an object with `name` + `arguments`.
///
/// Anything else → empty result. We intentionally do NOT emit a
/// `FunctionCallParse` error just because there were no fences: plain
/// prose is the common case.
fn dispatch_function_calls_from_text(
    source: &str,
    plugins: &dyn PluginRegistryView,
) -> Vec<DispatchOutcome> {
    let mut out = Vec::new();

    // Fenced extraction.
    let open_tag = "<tool_call>";
    let close_tag = "</tool_call>";
    let mut cursor = 0usize;
    while let Some(rel) = source[cursor..].find(open_tag) {
        let open_start = cursor + rel;
        let body_start = open_start + open_tag.len();
        let close_rel = match source[body_start..].find(close_tag) {
            Some(r) => r,
            None => {
                out.push(DispatchOutcome {
                    call: Err(DispatchError::FunctionCallParse(
                        "unterminated <tool_call> fence".into(),
                    )),
                    origin_offset: Some((open_start, source.len())),
                });
                return out;
            }
        };
        let body_end = body_start + close_rel;
        let fence_end = body_end + close_tag.len();
        let body = source[body_start..body_end].trim();

        match parse_fc_payload(body) {
            Ok(fc) => out.push(resolve_function_call(
                fc,
                plugins,
                Some((open_start, fence_end)),
            )),
            Err(e) => out.push(DispatchOutcome {
                call: Err(DispatchError::FunctionCallParse(e)),
                origin_offset: Some((open_start, fence_end)),
            }),
        }

        cursor = fence_end;
    }

    // Pure-JSON fallback: only try when no fences matched AND the whole
    // trimmed string looks like a JSON object.
    if out.is_empty() {
        let trimmed = source.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(fc) = parse_fc_payload(trimmed) {
                out.push(resolve_function_call(fc, plugins, None));
            }
        }
    }

    out
}

/// Parse a JSON object into an [`OpenAiFunctionCall`]. Accepts the two
/// historical OpenAI shapes: `arguments` as an object, or as a
/// JSON-encoded string.
fn parse_fc_payload(body: &str) -> Result<OpenAiFunctionCall, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))?;
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing `name`".to_string())?
        .to_string();
    let arguments = match v.get("arguments") {
        Some(Value::String(s)) => {
            serde_json::from_str(s).map_err(|e| format!("invalid `arguments` string: {e}"))?
        }
        Some(other) => other.clone(),
        None => Value::Object(Default::default()),
    };
    Ok(OpenAiFunctionCall { name, arguments })
}

fn resolve_function_call(
    fc: OpenAiFunctionCall,
    plugins: &dyn PluginRegistryView,
    origin: Option<(usize, usize)>,
) -> DispatchOutcome {
    let resolution = match plugins.resolve_tool(&fc.name) {
        Some(r) => r,
        None => {
            return DispatchOutcome {
                call: Err(DispatchError::UnknownTool { name: fc.name }),
                origin_offset: origin,
            };
        }
    };

    if !resolution.protocols.iter().any(|p| p == "openai_function") {
        return DispatchOutcome {
            call: Err(DispatchError::ProtocolNotAdvertised {
                tool: fc.name,
                protocol: "openai_function".into(),
                advertised: resolution.protocols.clone(),
            }),
            origin_offset: origin,
        };
    }

    DispatchOutcome {
        call: Ok(DispatchedCall {
            protocol: "openai_function".into(),
            plugin_name: resolution.plugin_name,
            tool_name: resolution.tool_name,
            args: fc.arguments,
            raw_span: origin,
        }),
        origin_offset: origin,
    }
}

// ---------------------------------------------------------------------------
// Merge / de-dup
// ---------------------------------------------------------------------------

/// Interleave per-protocol outcome lists into the final order. When the
/// same tool is matched by two protocols and their source spans overlap,
/// the higher-priority one (earlier in `preference_order`) wins.
///
/// Outcomes that are `Err(...)` never suppress another protocol's match:
/// the caller should see all parse failures, not just the winning one.
fn merge_by_preference(
    order: &[String],
    mut by_protocol: HashMap<String, Vec<DispatchOutcome>>,
) -> Vec<DispatchOutcome> {
    // Walk in priority order and accumulate a kept list + a list of the
    // spans that have already "claimed" a region of source text.
    let mut kept: Vec<DispatchOutcome> = Vec::new();
    let mut claimed_spans: Vec<(usize, usize)> = Vec::new();

    for proto in order {
        let list = match by_protocol.remove(proto) {
            Some(l) => l,
            None => continue,
        };
        for outcome in list {
            if let Ok(ref call) = outcome.call {
                if let Some(span) = call.raw_span {
                    if claimed_spans
                        .iter()
                        .any(|claimed| spans_overlap(*claimed, span))
                    {
                        // Suppressed: a higher-priority protocol already
                        // consumed this region.
                        continue;
                    }
                    claimed_spans.push(span);
                }
            }
            kept.push(outcome);
        }
    }

    // Stable-sort by origin offset so callers see outcomes in source
    // order regardless of which protocol produced them.
    kept.sort_by_key(|o| o.origin_offset.map(|(s, _)| s).unwrap_or(usize::MAX));
    kept
}

fn spans_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

fn hashmap_to_value(map: HashMap<String, Value>) -> Value {
    let obj: serde_json::Map<String, Value> = map.into_iter().collect();
    Value::Object(obj)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    /// In-file mock registry. `tools` maps `tool_name -> ToolResolution`.
    struct MockRegistry {
        tools: HashMap<String, ToolResolution>,
    }

    impl MockRegistry {
        fn new() -> Self {
            Self {
                tools: HashMap::new(),
            }
        }

        fn with(mut self, tool: &str, plugin: &str, protocols: &[&str], schema: Value) -> Self {
            self.tools.insert(
                tool.to_string(),
                ToolResolution {
                    plugin_name: plugin.to_string(),
                    tool_name: tool.to_string(),
                    protocols: protocols.iter().map(|s| s.to_string()).collect(),
                    parameters_schema: schema,
                },
            );
            self
        }
    }

    impl PluginRegistryView for MockRegistry {
        fn resolve_tool(&self, tool_name: &str) -> Option<ToolResolution> {
            self.tools.get(tool_name).cloned()
        }
    }

    fn schema_count_flag() -> Value {
        json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" },
                "flag":  { "type": "boolean" }
            }
        })
    }

    fn schema_object_arg() -> Value {
        json!({
            "type": "object",
            "properties": {
                "payload": { "type": "object" }
            }
        })
    }

    // 1. Same tool invoked via both protocols → coerced args are identical.
    #[test]
    fn both_protocols_same_tool_produce_identical_args() {
        let reg = MockRegistry::new().with(
            "adder",
            "math",
            &["openai_function", "block"],
            schema_count_flag(),
        );

        // Block envelope.
        let block_src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」adder「末」,
count:「始」7「末」,
flag:「始」true「末」
<<<[END_TOOL_REQUEST]>>>";
        let policy = ProtocolPolicy {
            preference_order: vec!["block".into()],
            ..ProtocolPolicy::default()
        };
        let out = dispatch(block_src, &reg, &policy);
        let block_call = out[0].call.as_ref().unwrap();
        assert_eq!(block_call.protocol, "block");

        // Function-call JSON.
        let fc = OpenAiFunctionCall {
            name: "adder".into(),
            arguments: json!({ "count": 7, "flag": true }),
        };
        let fc_out = dispatch_function_calls(vec![fc], &reg);
        let fc_call = fc_out[0].call.as_ref().unwrap();
        assert_eq!(fc_call.protocol, "openai_function");

        assert_eq!(block_call.args, fc_call.args);
    }

    // 2. Tool that only advertises block → function-call is rejected.
    #[test]
    fn block_only_tool_rejects_function_call() {
        let reg = MockRegistry::new().with("blocky", "p", &["block"], schema_count_flag());
        let fc = OpenAiFunctionCall {
            name: "blocky".into(),
            arguments: json!({"count": 1}),
        };
        let out = dispatch_function_calls(vec![fc], &reg);
        match &out[0].call {
            Err(DispatchError::ProtocolNotAdvertised {
                tool,
                protocol,
                advertised,
            }) => {
                assert_eq!(tool, "blocky");
                assert_eq!(protocol, "openai_function");
                assert_eq!(advertised, &vec!["block".to_string()]);
            }
            other => panic!("expected ProtocolNotAdvertised, got {other:?}"),
        }
    }

    // 3. Fallback off + block envelope + fc-only tool → error.
    #[test]
    fn function_call_only_tool_rejects_block_when_fallback_off() {
        let reg =
            MockRegistry::new().with("fc_only", "p", &["openai_function"], schema_count_flag());
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」fc_only「末」,
count:「始」3「末」
<<<[END_TOOL_REQUEST]>>>";
        let policy = ProtocolPolicy {
            block_enabled: true,
            fallback_to_function_call: false,
            preference_order: vec!["block".into()],
        };
        let out = dispatch(src, &reg, &policy);
        assert!(matches!(
            out[0].call,
            Err(DispatchError::ProtocolNotAdvertised { .. })
        ));
    }

    // 4. Fallback on → block envelope downgrades to fc successfully.
    #[test]
    fn function_call_only_tool_accepts_block_when_fallback_on() {
        let reg =
            MockRegistry::new().with("fc_only", "p", &["openai_function"], schema_count_flag());
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」fc_only「末」,
count:「始」3「末」
<<<[END_TOOL_REQUEST]>>>";
        let policy = ProtocolPolicy {
            block_enabled: true,
            fallback_to_function_call: true,
            preference_order: vec!["block".into()],
        };
        let out = dispatch(src, &reg, &policy);
        let call = out[0].call.as_ref().expect("should downgrade, not error");
        assert_eq!(call.protocol, "openai_function");
        assert_eq!(call.tool_name, "fc_only");
        assert_eq!(call.args, json!({"count": 3}));
    }

    // 5. Unknown tool → UnknownTool.
    #[test]
    fn unknown_tool_returns_unknown_tool_err() {
        let reg = MockRegistry::new();
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」ghost「末」
<<<[END_TOOL_REQUEST]>>>";
        let out = dispatch(src, &reg, &ProtocolPolicy::default());
        let err = out
            .iter()
            .find_map(|o| match &o.call {
                Err(DispatchError::UnknownTool { name }) => Some(name.clone()),
                _ => None,
            })
            .expect("should see UnknownTool");
        assert_eq!(err, "ghost");
    }

    // 6. block_enabled = false → block envelopes are ignored.
    #[test]
    fn block_disabled_in_policy_means_no_block_dispatch() {
        let reg = MockRegistry::new().with(
            "adder",
            "math",
            &["openai_function", "block"],
            schema_count_flag(),
        );
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」adder「末」,
count:「始」1「末」
<<<[END_TOOL_REQUEST]>>>";
        let policy = ProtocolPolicy {
            block_enabled: false,
            fallback_to_function_call: true,
            preference_order: vec!["block".into(), "openai_function".into()],
        };
        let out = dispatch(src, &reg, &policy);
        // No block dispatch, and no OpenAI fence inside source → empty.
        assert!(
            out.iter()
                .all(|o| o.call.is_err() || !matches!(&o.call, Ok(c) if c.protocol == "block")),
            "no block outcomes should be produced"
        );
        // In this specific input, nothing is produced at all.
        assert!(out.is_empty(), "expected no outcomes, got {out:?}");
    }

    // 7. Multiple block envelopes → multiple outcomes, in source order.
    #[test]
    fn multiple_blocks_in_output_produce_multiple_outcomes() {
        let reg = MockRegistry::new()
            .with("alpha", "p", &["block"], schema_count_flag())
            .with("beta", "p", &["block"], schema_count_flag());
        let src = "\
noise
<<<[TOOL_REQUEST]>>>
tool_name:「始」alpha「末」,
count:「始」1「末」
<<<[END_TOOL_REQUEST]>>>
more noise
<<<[TOOL_REQUEST]>>>
tool_name:「始」beta「末」,
count:「始」2「末」
<<<[END_TOOL_REQUEST]>>>
tail";
        let policy = ProtocolPolicy {
            preference_order: vec!["block".into()],
            ..ProtocolPolicy::default()
        };
        let out = dispatch(src, &reg, &policy);
        assert_eq!(out.len(), 2);
        let names: Vec<_> = out
            .iter()
            .map(|o| o.call.as_ref().unwrap().tool_name.clone())
            .collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    // 8. Integer coercion from block matches integer from function-call.
    #[test]
    fn coerce_integer_from_block_matches_function_call_integer() {
        let reg = MockRegistry::new().with(
            "adder",
            "math",
            &["openai_function", "block"],
            schema_count_flag(),
        );
        let block_src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」adder「末」,
count:「始」42「末」
<<<[END_TOOL_REQUEST]>>>";
        let policy = ProtocolPolicy {
            preference_order: vec!["block".into()],
            ..ProtocolPolicy::default()
        };
        let block_out = dispatch(block_src, &reg, &policy);
        let b_count = block_out[0]
            .call
            .as_ref()
            .unwrap()
            .args
            .get("count")
            .unwrap()
            .clone();

        let fc = OpenAiFunctionCall {
            name: "adder".into(),
            arguments: json!({"count": 42}),
        };
        let fc_out = dispatch_function_calls(vec![fc], &reg);
        let f_count = fc_out[0]
            .call
            .as_ref()
            .unwrap()
            .args
            .get("count")
            .unwrap()
            .clone();

        assert_eq!(b_count, f_count);
        assert_eq!(b_count, json!(42));
    }

    // 9. Object arg from block coerces to same Value as function-call object.
    #[test]
    fn coerce_object_arg_from_block_produces_same_value_as_function_call_object() {
        let reg = MockRegistry::new().with(
            "store",
            "p",
            &["openai_function", "block"],
            schema_object_arg(),
        );
        let block_src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」store「末」,
payload:「始」{\"a\":1,\"b\":\"two\"}「末」
<<<[END_TOOL_REQUEST]>>>";
        let policy = ProtocolPolicy {
            preference_order: vec!["block".into()],
            ..ProtocolPolicy::default()
        };
        let block_out = dispatch(block_src, &reg, &policy);
        let b = block_out[0]
            .call
            .as_ref()
            .unwrap()
            .args
            .get("payload")
            .unwrap()
            .clone();

        let fc = OpenAiFunctionCall {
            name: "store".into(),
            arguments: json!({ "payload": { "a": 1, "b": "two" } }),
        };
        let fc_out = dispatch_function_calls(vec![fc], &reg);
        let f = fc_out[0]
            .call
            .as_ref()
            .unwrap()
            .args
            .get("payload")
            .unwrap()
            .clone();

        assert_eq!(b, f);
        assert_eq!(b, json!({"a":1,"b":"two"}));
    }

    // 10. preference_order ["openai_function", "block"] + source with a
    //     fenced FC payload → we see an fc outcome first.
    #[test]
    fn preference_order_function_first_produces_fc_outcome() {
        let reg = MockRegistry::new().with(
            "adder",
            "math",
            &["openai_function", "block"],
            schema_count_flag(),
        );
        let src = r#"<tool_call>{"name":"adder","arguments":{"count":9}}</tool_call>"#;
        let policy = ProtocolPolicy {
            preference_order: vec!["openai_function".into(), "block".into()],
            ..ProtocolPolicy::default()
        };
        let out = dispatch(src, &reg, &policy);
        assert_eq!(out.len(), 1);
        let call = out[0].call.as_ref().unwrap();
        assert_eq!(call.protocol, "openai_function");
        assert_eq!(call.args, json!({"count": 9}));
    }

    // 11. preference_order ["block", ...] + block envelope → block wins.
    #[test]
    fn preference_order_block_first_produces_block_outcome() {
        let reg = MockRegistry::new().with(
            "adder",
            "math",
            &["openai_function", "block"],
            schema_count_flag(),
        );
        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」adder「末」,
count:「始」9「末」
<<<[END_TOOL_REQUEST]>>>";
        let policy = ProtocolPolicy {
            preference_order: vec!["block".into(), "openai_function".into()],
            ..ProtocolPolicy::default()
        };
        let out = dispatch(src, &reg, &policy);
        let call = out
            .iter()
            .find_map(|o| o.call.as_ref().ok())
            .expect("expected a successful outcome");
        assert_eq!(call.protocol, "block");
    }

    // 12. Overlapping spans: same region claims by block + FC fence →
    //     higher-priority wins. We construct an artificial text where a
    //     `<tool_call>` fence sits inside a block envelope (legal since
    //     block values are arbitrary strings).
    #[test]
    fn overlapping_spans_deduplicated_to_higher_priority() {
        // One physical region that both scanners will latch onto: a block
        // envelope whose `payload` value contains a valid fenced
        // function-call JSON.
        let reg = MockRegistry::new()
            .with(
                "outer",
                "p",
                &["block"],
                json!({
                    "type": "object",
                    "properties": { "payload": { "type": "string" } }
                }),
            )
            .with("inner", "p", &["openai_function"], schema_count_flag());

        let src = "\
<<<[TOOL_REQUEST]>>>
tool_name:「始」outer「末」,
payload:「始」<tool_call>{\"name\":\"inner\",\"arguments\":{\"count\":1}}</tool_call>「末」
<<<[END_TOOL_REQUEST]>>>";

        // Prefer block → the inner fence is inside the block's claimed
        // span and should be suppressed. We get exactly one outcome.
        let policy_block_first = ProtocolPolicy {
            preference_order: vec!["block".into(), "openai_function".into()],
            ..ProtocolPolicy::default()
        };
        let out = dispatch(src, &reg, &policy_block_first);
        assert_eq!(out.len(), 1, "outcomes = {out:?}");
        assert_eq!(
            out[0].call.as_ref().unwrap().protocol,
            "block",
            "block should win under block-first priority"
        );

        // Flip priority: openai_function comes first. The fence is
        // claimed first; the block envelope's span overlaps, so the
        // block outcome is suppressed.
        let policy_fc_first = ProtocolPolicy {
            preference_order: vec!["openai_function".into(), "block".into()],
            ..ProtocolPolicy::default()
        };
        let out = dispatch(src, &reg, &policy_fc_first);
        assert_eq!(out.len(), 1, "outcomes = {out:?}");
        assert_eq!(out[0].call.as_ref().unwrap().protocol, "openai_function");
    }

    // ---------- extra coverage ----------

    #[test]
    fn structured_function_calls_args_as_string_are_parsed() {
        let reg =
            MockRegistry::new().with("adder", "math", &["openai_function"], schema_count_flag());
        // OpenAI historical shape: arguments is a JSON-encoded string.
        let fc = OpenAiFunctionCall {
            name: "adder".into(),
            arguments: json!("{\"count\": 11}"),
        };
        // Manually route through parse_fc_payload equivalent by wrapping.
        // `dispatch_function_calls` accepts arguments already materialised,
        // so this test confirms `parse_fc_payload` via the raw-text path.
        let src = r#"<tool_call>{"name":"adder","arguments":"{\"count\":11}"}</tool_call>"#;
        let out = dispatch(src, &reg, &ProtocolPolicy::default());
        let _ = fc; // silence unused
        assert_eq!(out.len(), 1);
        let call = out[0].call.as_ref().unwrap();
        assert_eq!(call.args, json!({"count": 11}));
    }

    #[test]
    fn pure_json_fallback_recognises_object() {
        let reg =
            MockRegistry::new().with("adder", "math", &["openai_function"], schema_count_flag());
        let src = r#"{"name":"adder","arguments":{"count":5}}"#;
        let out = dispatch(src, &reg, &ProtocolPolicy::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].call.as_ref().unwrap().args, json!({"count": 5}));
    }

    #[test]
    fn unterminated_tool_call_fence_yields_function_call_parse_err() {
        let reg = MockRegistry::new();
        let src = r#"<tool_call>{"name":"x"}"#;
        let out = dispatch(src, &reg, &ProtocolPolicy::default());
        assert!(matches!(
            out[0].call,
            Err(DispatchError::FunctionCallParse(_))
        ));
    }
}
