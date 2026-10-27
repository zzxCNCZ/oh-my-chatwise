use super::{Registry, transport, types::*};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Configuration {
    pub proxy: super::proxy::Configuration,
    pub clock: bool,
    pub calculator: bool,
    pub brave: super::brave::Configuration,
    pub http: Vec<HttpTool>,
    pub mcp: Vec<McpServer>,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            proxy: Default::default(),
            clock: true,
            calculator: true,
            brave: super::brave::Configuration::default(),
            http: vec![],
            mcp: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpTool {
    pub definition: ToolDefinition,
    pub enabled: bool,
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub query: BTreeMap<String, Value>,
    #[serde(default)]
    pub body: Value,
    #[serde(default)]
    pub response_path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub tools: Vec<McpTool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub enabled: bool,
    pub risk: Risk,
}
impl Configuration {
    pub fn configured_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if self.brave.active() {
            names.push("brave_search".into());
        }
        names.extend(
            self.http
                .iter()
                .filter(|t| t.enabled)
                .map(|t| t.definition.name.clone()),
        );
        for server in self.mcp.iter().filter(|s| s.enabled) {
            names.extend(
                server
                    .tools
                    .iter()
                    .filter(|t| t.enabled)
                    .map(|t| mcp_name(&server.id, &t.name)),
            );
        }
        names
    }
    pub fn validate(&self) -> Result<(), String> {
        self.proxy.validate()?;
        self.brave.validate()?;
        if serde_json::to_vec(self)
            .map_err(|_| "工具配置无效。")?
            .len()
            > 256 * 1024
            || self.http.len() > 32
            || self.mcp.len() > 8
        {
            return Err("工具配置上限 256 KB、32 个 HTTP 工具及 8 个 MCP 服务器。".into());
        }
        let mut names = std::collections::BTreeSet::new();
        if self.clock {
            names.insert("current_time".to_string());
        }
        if self.calculator {
            names.insert("calculate".to_string());
        }
        if self.brave.active() {
            names.insert("brave_search".to_string());
        }
        for tool in &self.http {
            if !valid_name(&tool.definition.name)
                || !names.insert(tool.definition.name.clone())
                || tool.definition.input_schema["type"] != "object"
                || !matches!(
                    tool.method.as_str(),
                    "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
                )
            {
                return Err("HTTP 工具名称重复、Schema 或请求方法无效。".into());
            }
            transport::validate_endpoint(&tool.url, &tool.headers)?;
        }
        let mut server_ids = std::collections::BTreeSet::new();
        for server in &self.mcp {
            if uuid::Uuid::parse_str(&server.id).is_err()
                || !server_ids.insert(&server.id)
                || server.name.trim().is_empty()
                || server.tools.len() > 64
            {
                return Err("MCP 服务器 ID 重复或配置无效。".into());
            }
            transport::validate_endpoint(&server.url, &server.headers)?;
            let mut remote = std::collections::BTreeSet::new();
            for tool in &server.tools {
                if tool.name.is_empty()
                    || tool.name.len() > 256
                    || !remote.insert(&tool.name)
                    || tool.input_schema["type"] != "object"
                    || !names.insert(mcp_name(&server.id, &tool.name))
                {
                    return Err("MCP 工具名称或 Schema 无效。".into());
                }
            }
        }
        if names.len() > 96 {
            return Err("同时配置的工具总数不能超过 96。".into());
        }
        Ok(())
    }
    pub fn for_conversation(&self, selection: &crate::models::ConversationTools) -> Self {
        let mut config = self.clone();
        config.clock = true;
        config.calculator = true;
        config.brave.enabled &= selection.web;
        if !selection.http {
            config.http.clear();
        }
        if !selection.mcp {
            config.mcp.clear();
        }
        config
    }
    pub fn registry(&self, client: &reqwest::Client) -> Result<Registry, String> {
        self.validate()?;
        let client = self.proxy.client(client)?;
        let mut registry = Registry::default();
        if self.clock {
            registry.register(Arc::new(Builtin::clock()))?;
        }
        if self.calculator {
            registry.register(Arc::new(Builtin::calculator()))?;
        }
        if self.brave.active() {
            registry.register(Arc::new(super::brave::Executor {
                client: client.clone(),
                config: self.brave.clone(),
                definition: super::brave::definition(),
            }))?;
        }
        for tool in self.http.iter().filter(|t| t.enabled) {
            registry.register(Arc::new(HttpExecutor {
                client: client.clone(),
                config: tool.clone(),
            }))?;
        }
        for server in self.mcp.iter().filter(|s| s.enabled) {
            let remote = Arc::new(transport::McpClient::new(client.clone(), server.clone()));
            for tool in server.tools.iter().filter(|t| t.enabled) {
                registry.register(Arc::new(McpExecutor {
                    definition: ToolDefinition {
                        name: mcp_name(&server.id, &tool.name),
                        description: format!("{} / {}", server.name, tool.description),
                        input_schema: tool.input_schema.clone(),
                        risk: tool.risk.clone(),
                    },
                    remote_name: tool.name.clone(),
                    remote: remote.clone(),
                }))?;
            }
        }
        Ok(registry)
    }
}
pub fn mcp_name(server: &str, name: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, name.as_bytes());
    let suffix = digest.as_ref()[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let clean = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(36)
        .collect::<String>();
    format!(
        "mcp_{}_{}_{}",
        server.chars().take(8).collect::<String>(),
        clean,
        suffix
    )
}
struct HttpExecutor {
    client: reqwest::Client,
    config: HttpTool,
}
impl Tool for HttpExecutor {
    fn target(&self) -> Option<String> {
        Some(format!(
            "{} {}",
            self.config.method,
            transport::public_target(&self.config.url)
        ))
    }
    fn definition(&self) -> &ToolDefinition {
        &self.config.definition
    }
    fn execute<'a>(&'a self, args: &'a Value, _cancel: &'a CancellationToken) -> Task<'a, Value> {
        Box::pin(transport::http_tool(&self.client, &self.config, args))
    }
}
struct McpExecutor {
    definition: ToolDefinition,
    remote_name: String,
    remote: Arc<transport::McpClient>,
}
impl Tool for McpExecutor {
    fn target(&self) -> Option<String> {
        Some(self.remote.target())
    }
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }
    fn execute<'a>(&'a self, args: &'a Value, cancel: &'a CancellationToken) -> Task<'a, Value> {
        Box::pin(self.remote.call(&self.remote_name, args, cancel))
    }
}
struct Builtin {
    definition: ToolDefinition,
}
impl Builtin {
    fn clock() -> Self {
        Self {
            definition: ToolDefinition {
                name: "current_time".into(),
                description: "Return the current UTC time and Unix timestamp from this computer."
                    .into(),
                input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
                risk: Risk::ReadOnly,
            },
        }
    }
    fn calculator() -> Self {
        Self{definition:ToolDefinition{name:"calculate".into(),description:"Evaluate arithmetic with numbers, +, -, *, /, %, and parentheses. No code execution.".into(),input_schema:json!({"type":"object","properties":{"expression":{"type":"string","maxLength":1024}},"required":["expression"],"additionalProperties":false}),risk:Risk::ReadOnly}}
    }
}
impl Tool for Builtin {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }
    fn execute<'a>(&'a self, args: &'a Value, _cancel: &'a CancellationToken) -> Task<'a, Value> {
        Box::pin(async move {
            if self.definition.name == "calculate" {
                return Ok(
                    json!({"value":calculate(args["expression"].as_str().ok_or("缺少 expression。")?)?}),
                );
            }
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "系统时间无效。")?
                .as_secs();
            let days = seconds / 86400;
            let z = days as i64 + 719468;
            let era = z / 146097;
            let doe = z - era * 146097;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let mut year = yoe + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let day = doy - (153 * mp + 2) / 5 + 1;
            let month = mp + if mp < 10 { 3 } else { -9 };
            year += i64::from(month <= 2);
            Ok(
                json!({"unixSeconds":seconds,"utc":format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",seconds%86400/3600,seconds%3600/60,seconds%60)}),
            )
        })
    }
}
pub fn validate_arguments(args: &Value, schema: &Value) -> Result<(), String> {
    fn visit(value: &Value, schema: &Value, depth: u8) -> Result<(), String> {
        if depth > 32 {
            return Err("工具参数嵌套过深。".into());
        }
        if schema == &Value::Bool(false) {
            return Err("Schema 不允许该参数。".into());
        }
        if let Some(options) = schema["enum"].as_array()
            && !options.contains(value)
        {
            return Err("参数不在枚举范围内。".into());
        }
        if let Some(options) = schema["anyOf"].as_array() {
            if options.iter().any(|s| visit(value, s, depth + 1).is_ok()) {
                return Ok(());
            }
            return Err("参数不符合 anyOf。".into());
        }
        let matches = |kind: &str| match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if let Some(kind) = schema.get("type") {
            let valid = kind
                .as_str()
                .map(matches)
                .or_else(|| {
                    kind.as_array()
                        .map(|a| a.iter().filter_map(Value::as_str).any(matches))
                })
                .unwrap_or(false);
            if !valid {
                return Err("工具参数类型不符合 Schema。".into());
            }
        }
        if let Some(object) = value.as_object() {
            if let Some(required) = schema["required"].as_array() {
                for key in required {
                    if !object.contains_key(key.as_str().ok_or("Schema required 无效。")?) {
                        return Err(format!("缺少必填参数：{}", key));
                    }
                }
            }
            for (key, value) in object {
                if let Some(property) = schema["properties"].get(key) {
                    visit(value, property, depth + 1)?;
                } else if schema["additionalProperties"] == false {
                    return Err(format!("未知参数：{key}"));
                }
            }
        }
        if let Some(items) = value.as_array() {
            if schema["maxItems"]
                .as_u64()
                .is_some_and(|max| items.len() as u64 > max)
            {
                return Err("数组参数过长。".into());
            }
            if let Some(item_schema) = schema.get("items") {
                for value in items {
                    visit(value, item_schema, depth + 1)?;
                }
            }
        }
        if let Some(text) = value.as_str()
            && schema["maxLength"]
                .as_u64()
                .is_some_and(|max| text.chars().count() as u64 > max)
        {
            return Err("字符串参数过长。".into());
        }
        if let Some(number) = value.as_f64()
            && (schema["minimum"].as_f64().is_some_and(|min| number < min)
                || schema["maximum"].as_f64().is_some_and(|max| number > max))
        {
            return Err("数值参数超出范围。".into());
        }
        Ok(())
    }
    if !args.is_object() || serde_json::to_vec(args).map_err(|_| "无效参数。")?.len() > 64 * 1024
    {
        return Err("工具参数必须是 64 KB 以内的 JSON 对象。".into());
    }
    visit(args, schema, 0)
}
fn calculate(expression: &str) -> Result<f64, String> {
    struct Parser {
        bytes: Vec<u8>,
        pos: usize,
    }
    impl Parser {
        fn expr(&mut self, depth: u8) -> Result<f64, String> {
            if depth > 32 {
                return Err("表达式嵌套过深。".into());
            }
            let mut value = self.term(depth + 1)?;
            while let Some(op @ (b'+' | b'-')) = self.bytes.get(self.pos).copied() {
                self.pos += 1;
                let rhs = self.term(depth + 1)?;
                value = if op == b'+' { value + rhs } else { value - rhs };
            }
            Ok(value)
        }
        fn term(&mut self, depth: u8) -> Result<f64, String> {
            let mut value = self.factor(depth + 1)?;
            while let Some(op @ (b'*' | b'/' | b'%')) = self.bytes.get(self.pos).copied() {
                self.pos += 1;
                let rhs = self.factor(depth + 1)?;
                if rhs == 0.0 && op != b'*' {
                    return Err("不能除以零。".into());
                }
                value = match op {
                    b'*' => value * rhs,
                    b'/' => value / rhs,
                    _ => value % rhs,
                };
            }
            Ok(value)
        }
        fn factor(&mut self, depth: u8) -> Result<f64, String> {
            if depth > 64 {
                return Err("表达式嵌套过深。".into());
            }
            if matches!(self.bytes.get(self.pos), Some(b'+') | Some(b'-')) {
                let negative = self.bytes[self.pos] == b'-';
                self.pos += 1;
                return self
                    .factor(depth + 1)
                    .map(|v| if negative { -v } else { v });
            }
            if self.bytes.get(self.pos) == Some(&b'(') {
                self.pos += 1;
                let value = self.expr(depth + 1)?;
                if self.bytes.get(self.pos) != Some(&b')') {
                    return Err("括号不匹配。".into());
                }
                self.pos += 1;
                return Ok(value);
            }
            let start = self.pos;
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
            {
                self.pos += 1;
            }
            std::str::from_utf8(&self.bytes[start..self.pos])
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "无效数字或不支持的运算符。".into())
        }
    }
    if expression.len() > 1024 {
        return Err("表达式过长。".into());
    }
    let mut p = Parser {
        bytes: expression
            .bytes()
            .filter(|c| !c.is_ascii_whitespace())
            .collect(),
        pos: 0,
    };
    let value = p.expr(0)?;
    if p.pos != p.bytes.len() || !value.is_finite() {
        return Err("表达式无效或结果超出有限数值范围。".into());
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calculator_and_schema_are_bounded() {
        assert_eq!(calculate("(3+5)*2-4/2").unwrap(), 14.0);
        assert!(calculate("1/0").is_err());
        assert!(calculate("system('anything')").is_err());
        assert!(calculate(&"(".repeat(200)).is_err());
        assert!(
            validate_arguments(
                &json!({"expression":12}),
                &Builtin::calculator().definition.input_schema
            )
            .is_err()
        );
        assert!(
            validate_arguments(
                &json!({"extra":1}),
                &Builtin::clock().definition.input_schema
            )
            .is_err()
        );
    }
}
