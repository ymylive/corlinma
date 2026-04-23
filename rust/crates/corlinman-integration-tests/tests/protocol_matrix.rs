//! B3 cross-workstream contract test: dual-protocol parity matrix.
//!
//! For the same logical tool invocation, the block-protocol envelope and
//! the OpenAI function-call envelope must produce byte-identical coerced
//! argument Values. This test pins the observable contract documented in
//! `docs/protocols/b3-contracts.md` (dispatcher protocol coercion
//! equivalence) so future changes to either parser know what NOT to break.
//!
//! Scope: this file intentionally only asserts arg-Value equality. The
//! per-protocol parse / error surface is already covered by the tests in
//! `corlinman-plugins::protocol::{block, dispatcher}`; we do not duplicate
//! that coverage here.

use std::collections::HashMap;

use corlinman_plugins::protocol::{
    dispatch, dispatch_function_calls, OpenAiFunctionCall, PluginRegistryView, ProtocolPolicy,
    ToolResolution,
};
use serde_json::{json, Value};

/// In-file mock registry — the dispatcher only needs `resolve_tool`.
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

/// One parity case. `block_input` is a full `<<<[TOOL_REQUEST]>>>`
/// envelope; `fc_input` is the structured function-call shape. The
/// coerced `.args` Values must be equal to `expected_args`.
struct Case {
    desc: &'static str,
    tool: &'static str,
    schema: Value,
    block_input: String,
    fc_input: OpenAiFunctionCall,
    expected_args: Value,
}

/// Build a case by supplying a tool schema + both envelope shapes.
fn case(
    desc: &'static str,
    tool: &'static str,
    schema: Value,
    block_input: impl Into<String>,
    fc_args: Value,
    expected_args: Value,
) -> Case {
    Case {
        desc,
        tool,
        schema,
        block_input: block_input.into(),
        fc_input: OpenAiFunctionCall {
            name: tool.to_string(),
            arguments: fc_args,
        },
        expected_args,
    }
}

/// Run one case through both dispatch paths and assert args equality.
fn run_case(c: &Case) {
    let reg = MockRegistry::new().with(
        c.tool,
        "matrix",
        &["openai_function", "block"],
        c.schema.clone(),
    );

    // Block path — prefer block protocol so we deterministically parse the envelope.
    let policy = ProtocolPolicy {
        preference_order: vec!["block".into(), "openai_function".into()],
        ..ProtocolPolicy::default()
    };
    let block_out = dispatch(&c.block_input, &reg, &policy);
    let block_call = block_out
        .iter()
        .find_map(|o| o.call.as_ref().ok())
        .unwrap_or_else(|| {
            panic!(
                "[{}] expected a successful block outcome, got: {:?}",
                c.desc, block_out
            )
        });
    assert_eq!(
        block_call.protocol, "block",
        "[{}] wrong protocol from block path: {:?}",
        c.desc, block_call
    );

    // Function-call path.
    let fc_out = dispatch_function_calls(vec![c.fc_input.clone()], &reg);
    let fc_call = fc_out[0].call.as_ref().unwrap_or_else(|_| {
        panic!(
            "[{}] expected a successful fc outcome, got: {:?}",
            c.desc, fc_out
        )
    });
    assert_eq!(fc_call.protocol, "openai_function", "[{}]", c.desc);

    // Parity: the two envelopes collapse to the same coerced args Value.
    assert_eq!(
        block_call.args, fc_call.args,
        "[{}] block args != fc args\n  block: {}\n  fc:    {}",
        c.desc, block_call.args, fc_call.args
    );
    assert_eq!(
        block_call.args, c.expected_args,
        "[{}] block args != expected\n  got:      {}\n  expected: {}",
        c.desc, block_call.args, c.expected_args
    );
}

#[test]
fn dual_protocol_parity_matrix() {
    let cases: Vec<Case> = vec![
        // 1. String arg: both envelopes carry a plain string value.
        case(
            "string arg",
            "echoer",
            json!({ "type": "object", "properties": { "msg": { "type": "string" } } }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」echoer「末」,\n\
             msg:「始」hello world「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            json!({ "msg": "hello world" }),
            json!({ "msg": "hello world" }),
        ),
        // 2. Integer arg: block coerces "42" → 42, fc sends 42 directly.
        case(
            "integer arg",
            "adder",
            json!({ "type": "object", "properties": { "count": { "type": "integer" } } }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」adder「末」,\n\
             count:「始」42「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            json!({ "count": 42 }),
            json!({ "count": 42 }),
        ),
        // 3. Boolean arg: block coerces "true" → true.
        case(
            "boolean arg",
            "toggler",
            json!({ "type": "object", "properties": { "flag": { "type": "boolean" } } }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」toggler「末」,\n\
             flag:「始」true「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            json!({ "flag": true }),
            json!({ "flag": true }),
        ),
        // 4. Nested object: block value is a JSON string, coerced to object.
        case(
            "nested object arg",
            "store",
            json!({
                "type": "object",
                "properties": { "payload": { "type": "object" } }
            }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」store「末」,\n\
             payload:「始」{\"a\":1,\"b\":{\"c\":2}}「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            json!({ "payload": { "a": 1, "b": { "c": 2 } } }),
            json!({ "payload": { "a": 1, "b": { "c": 2 } } }),
        ),
        // 5. Array arg: block JSON array coerced to Value::Array.
        case(
            "array arg",
            "picker",
            json!({
                "type": "object",
                "properties": { "items": { "type": "array" } }
            }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」picker「末」,\n\
             items:「始」[1, 2, 3]「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            json!({ "items": [1, 2, 3] }),
            json!({ "items": [1, 2, 3] }),
        ),
        // 6. Unicode CJK value inside a string arg — proves UTF-8 boundaries
        //    + serde_json parity for non-ASCII strings.
        case(
            "unicode CJK string value",
            "translator",
            json!({ "type": "object", "properties": { "text": { "type": "string" } } }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」translator「末」,\n\
             text:「始」你好，世界🌏「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            json!({ "text": "你好，世界🌏" }),
            json!({ "text": "你好，世界🌏" }),
        ),
        // 7. Trailing whitespace inside a numeric block value. `coerce_args`
        //    trims for integer/boolean/number types so "  7  " normalises to
        //    the same JSON Number the fc path sends as 7. Note this parity
        //    only holds for trimmed types — string types would preserve the
        //    whitespace verbatim and therefore differ from a trimmed fc value.
        case(
            "trailing whitespace in integer value",
            "adder",
            json!({ "type": "object", "properties": { "count": { "type": "integer" } } }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」adder「末」,\n\
             count:「始」  7  「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            json!({ "count": 7 }),
            json!({ "count": 7 }),
        ),
        // 8. Multi-arg: mix of types in one call. Proves block + fc agree on
        //    ordering-independent maps (serde_json::Value::Object is sorted
        //    by key on comparison when using assert_eq on PartialEq).
        case(
            "multi-arg (string + int + bool)",
            "runner",
            json!({
                "type": "object",
                "properties": {
                    "cmd":   { "type": "string"  },
                    "ttl":   { "type": "integer" },
                    "debug": { "type": "boolean" }
                }
            }),
            "<<<[TOOL_REQUEST]>>>\n\
             tool_name:「始」runner「末」,\n\
             cmd:「始」make build「末」,\n\
             ttl:「始」30「末」,\n\
             debug:「始」false「末」\n\
             <<<[END_TOOL_REQUEST]>>>",
            // fc arg order intentionally differs from block order to prove
            // that Value::Object comparison is order-independent.
            json!({ "debug": false, "ttl": 30, "cmd": "make build" }),
            json!({ "cmd": "make build", "ttl": 30, "debug": false }),
        ),
    ];

    assert_eq!(cases.len(), 8, "matrix must stay at 8 cases");
    for c in &cases {
        run_case(c);
    }
}
