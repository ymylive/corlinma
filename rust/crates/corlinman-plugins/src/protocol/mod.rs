//! Tool-call protocol parsers.
//!
//! Currently houses the structured-block parser used when a model emits
//! the `<<<[TOOL_REQUEST]>>> … <<<[END_TOOL_REQUEST]>>>` envelope instead
//! of an OpenAI function_call payload.

pub mod block;
pub mod dispatcher;

pub use dispatcher::{
    dispatch, dispatch_function_calls, DispatchError, DispatchOutcome, DispatchedCall,
    OpenAiFunctionCall, PluginRegistryView, ProtocolPolicy, ToolResolution,
};
