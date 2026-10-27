use crate::models::Protocol;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin};
use tokio_util::sync::CancellationToken;

pub type Task<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    Auto,
    On,
    Off,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentSettings {
    pub enabled: bool,
    pub mode: Option<AgentMode>,
    pub max_steps: u32,
    pub max_tool_calls: u32,
    pub tool_timeout_seconds: u64,
    pub run_timeout_seconds: u64,
    pub tool_call_limits: BTreeMap<String, u32>,
    pub tool_call_step_limits: BTreeMap<String, u32>,
}
impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: None,
            max_steps: 8,
            max_tool_calls: 16,
            tool_timeout_seconds: 30,
            run_timeout_seconds: 300,
            tool_call_limits: BTreeMap::new(),
            tool_call_step_limits: BTreeMap::new(),
        }
    }
}
impl AgentSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=32).contains(&self.max_steps)
            || !(1..=64).contains(&self.max_tool_calls)
            || !(1..=120).contains(&self.tool_timeout_seconds)
            || !(10..=1800).contains(&self.run_timeout_seconds)
        {
            return Err(
                "Agent 限额无效：轮次 1–32、调用数 1–64、工具超时 1–120 秒、总超时 10–1800 秒。"
                    .into(),
            );
        }
        for (name, count) in self
            .tool_call_limits
            .iter()
            .chain(&self.tool_call_step_limits)
        {
            if !valid_name(name) || *count == 0 || *count > 64 {
                return Err("单工具调用限额无效。".into());
            }
        }
        for (name, count) in &self.tool_call_step_limits {
            if self
                .tool_call_limits
                .get(name)
                .is_some_and(|limit| count > limit)
            {
                return Err("单轮限额不能超过该工具的整轮对话限额。".into());
            }
        }
        Ok(())
    }
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Risk {
    ReadOnly,
    ExternalAction,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub risk: Risk,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub content: String,
    pub status: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStep {
    pub number: u32,
    pub protocol: Protocol,
    #[serde(default)]
    pub model_key: String,
    pub content: String,
    pub reasoning: String,
    pub calls: Vec<ToolCall>,
    pub results: Vec<ToolResult>,
    pub status: String,
    #[serde(default)]
    pub provider_items: Vec<Value>,
}
#[derive(Default)]
pub struct ModelResponse {
    pub content: String,
    pub reasoning: String,
    pub calls: Vec<ToolCall>,
    pub provider_items: Vec<Value>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
pub type Emit<'a> = dyn FnMut(crate::models::StreamEvent) -> Result<(), String> + Send + 'a;
pub trait ModelClient: Send + Sync {
    fn protocol(&self) -> Protocol;
    fn identity(&self) -> String {
        String::new()
    }
    fn generate<'a>(
        &'a self,
        steps: &'a [AgentStep],
        tools: &'a [ToolDefinition],
        emit: &'a mut Emit<'_>,
    ) -> Task<'a, ModelResponse>;
}
pub trait Tool: Send + Sync {
    fn definition(&self) -> &ToolDefinition;
    fn target(&self) -> Option<String> {
        None
    }
    fn execute<'a>(
        &'a self,
        arguments: &'a Value,
        cancellation: &'a CancellationToken,
    ) -> Task<'a, Value>;
}
pub trait ToolPolicy: Send + Sync {
    fn approve<'a>(
        &'a self,
        call: &'a ToolCall,
        definition: &'a ToolDefinition,
        target: Option<&'a str>,
        cancellation: &'a CancellationToken,
    ) -> Task<'a, bool>;
}
pub struct ReadOnlyPolicy;
impl ToolPolicy for ReadOnlyPolicy {
    fn approve<'a>(
        &'a self,
        _call: &'a ToolCall,
        definition: &'a ToolDefinition,
        _target: Option<&'a str>,
        _cancellation: &'a CancellationToken,
    ) -> Task<'a, bool> {
        Box::pin(async move { Ok(definition.risk == Risk::ReadOnly) })
    }
}
