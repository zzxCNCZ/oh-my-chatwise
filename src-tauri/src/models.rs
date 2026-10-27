use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    ChatCompletions,
    Responses,
    Anthropic,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_reasoning: BTreeMap<String, ReasoningEffort>,
    #[serde(default)]
    pub has_api_key: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Default,
    Off,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub id: String,
    pub name: String,
    pub content: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ConversationTools {
    pub web: bool,
    pub http: bool,
    pub mcp: bool,
}
impl Default for ConversationTools {
    fn default() -> Self {
        Self {
            web: true,
            http: true,
            mcp: true,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationSettings {
    pub max_tokens: u32,
    pub temperature: Option<f64>,
    pub agent: crate::agent::types::AgentSettings,
}
impl Default for GenerationSettings {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            temperature: None,
            agent: Default::default(),
        }
    }
}
impl GenerationSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=128000).contains(&self.max_tokens)
            || self
                .temperature
                .is_some_and(|v| !v.is_finite() || !(0.0..=2.0).contains(&v))
        {
            return Err("全局生成参数无效。".into());
        }
        self.agent.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AvailableModel {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachment {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub size: u64,
    pub source: String,
    #[serde(default)]
    pub local: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip)]
    pub data: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextAttachment {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub format: String,
    pub context: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSettings {
    pub mode: String,
    pub has_upload_config: bool,
}
impl Default for ImageSettings {
    fn default() -> Self {
        Self {
            mode: "base64".into(),
            has_upload_config: false,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageUploadConfig {
    pub url: String,
    pub method: String,
    pub body_type: String,
    pub file_field: String,
    pub headers: BTreeMap<String, String>,
    pub body: serde_json::Value,
    pub response_type: String,
    pub response_url_path: String,
    pub public_url_template: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_steps: Vec<crate::agent::types::AgentStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_attachments: Vec<TextAttachment>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_generation: Option<TitleGeneration>,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub updated_at: String,
    pub pinned: bool,
    pub connection_id: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ConversationTools>,
    pub system_prompt: String,
    pub max_tokens: u32,
    pub temperature: Option<f64>,
    #[serde(default)]
    pub agent: crate::agent::types::AgentSettings,
    #[serde(default)]
    pub guide: bool,
    #[serde(default)]
    pub draft: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draft_images: Vec<ImageAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draft_texts: Vec<TextAttachment>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleGeneration {
    pub source: String,
    pub attempts: u32,
    pub last_attempt_turn: u32,
    pub summarized_turns: u32,
    pub finalized: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub version: u8,
    pub connections: Vec<Connection>,
    pub conversations: Vec<Conversation>,
    pub active_conversation_id: Option<String>,
    pub default_connection_id: String,
    pub default_model: String,
    pub theme: String,
    #[serde(default)]
    pub image_settings: ImageSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<GenerationSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<Prompt>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_prompt_id: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_steps: Vec<crate::agent::types::AgentStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_attachments: Vec<TextAttachment>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub id: String,
    pub connection: Connection,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub tools: ConversationTools,
    pub system_prompt: String,
    pub max_tokens: u32,
    pub temperature: Option<f64>,
    #[serde(default)]
    pub agent: crate::agent::types::AgentSettings,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StreamEvent {
    AgentStep {
        step: crate::agent::types::AgentStep,
    },
    ToolStarted {
        call_id: String,
    },
    ToolApproval {
        approval_id: String,
        call: crate::agent::types::ToolCall,
        description: String,
        target: String,
    },
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Usage {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn legacy_conversations_keep_missing_selection_fields_for_frontend_migration() {
        let legacy = serde_json::json!({"id":"old","title":"Old","messages":[],"createdAt":"now","updatedAt":"now","pinned":false,"connectionId":"","model":"","systemPrompt":"Keep legacy prompt","maxTokens":8000,"temperature":0.4,"agent":{"mode":"off"}});
        let chat: Conversation = serde_json::from_value(legacy).unwrap();
        let roundtrip = serde_json::to_value(chat).unwrap();
        assert!(roundtrip.get("tools").is_none());
        assert!(roundtrip.get("promptId").is_none());
        assert_eq!(roundtrip["systemPrompt"], "Keep legacy prompt");
        assert_eq!(roundtrip["agent"]["mode"], "off");
        let request: ChatRequest = serde_json::from_value(serde_json::json!({"id":"run","connection":{"id":"c","name":"c","protocol":"responses","baseUrl":"https://api.invalid","models":["m"]},"model":"m","messages":[],"systemPrompt":"","maxTokens":4096,"temperature":null})).unwrap();
        assert!(request.tools.web && request.tools.http && request.tools.mcp);
    }
}
