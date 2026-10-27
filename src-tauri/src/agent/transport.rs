use super::tools::{HttpTool, McpServer, McpTool};
use super::types::Risk;
use crate::sse::SseDecoder;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub fn public_target(value: &str) -> String {
    reqwest::Url::parse(value)
        .map(|mut url| {
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        })
        .unwrap_or_else(|_| "无效目标".into())
}
fn sanitized(
    mut value: Value,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&Value>,
    query: Option<&BTreeMap<String, Value>>,
) -> Value {
    fn auth_field(key: &str) -> bool {
        let key = key.to_ascii_lowercase().replace(['_', '-'], "");
        [
            "apikey",
            "token",
            "accesstoken",
            "secret",
            "password",
            "authorization",
            "key",
        ]
        .contains(&key.as_str())
    }
    fn collect(value: &Value, secrets: &mut Vec<String>) {
        if let Some(object) = value.as_object() {
            for (key, value) in object {
                if auth_field(key)
                    && let Some(text) = value.as_str()
                    && text.len() >= 4
                    && !text.contains("{{")
                {
                    secrets.push(text.into());
                }
                collect(value, secrets);
            }
        } else if let Some(array) = value.as_array() {
            for value in array {
                collect(value, secrets);
            }
        }
    }
    fn redact(value: &mut Value, secrets: &[String]) {
        match value {
            Value::String(text) => {
                for secret in secrets {
                    *text = text.replace(secret, "[已隐藏鉴权]");
                }
            }
            Value::Array(array) => {
                for v in array {
                    redact(v, secrets)
                }
            }
            Value::Object(object) => {
                for v in object.values_mut() {
                    redact(v, secrets)
                }
            }
            _ => {}
        }
    }
    let mut secrets = headers
        .values()
        .filter(|v| v.len() >= 8)
        .cloned()
        .collect::<Vec<_>>();
    for value in headers.values() {
        if let Some(token) = value.strip_prefix("Bearer ")
            && token.len() >= 4
        {
            secrets.push(token.into());
        }
    }
    if let Ok(url) = reqwest::Url::parse(url) {
        for (key, value) in url.query_pairs() {
            if auth_field(&key) && value.len() >= 4 {
                secrets.push(value.into_owned());
            }
        }
    }
    if let Some(body) = body {
        collect(body, &mut secrets);
    }
    if let Some(query) = query {
        for (key, value) in query {
            if auth_field(key)
                && let Some(text) = value.as_str()
                && text.len() >= 4
                && !text.contains("{{")
            {
                secrets.push(text.into());
            }
        }
    }
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    redact(&mut value, &secrets);
    value
}
pub fn validate_endpoint(
    url: &str,
    headers: &BTreeMap<String, String>,
) -> Result<reqwest::Url, String> {
    let url = crate::attachments::validate_url(url)?;
    for (key, value) in headers {
        reqwest::header::HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| "工具 Header 名无效。")?;
        reqwest::header::HeaderValue::from_str(value).map_err(|_| "工具 Header 值无效。")?;
        if [
            "host",
            "content-length",
            "transfer-encoding",
            "connection",
            "mcp-session-id",
            "mcp-protocol-version",
        ]
        .contains(&key.to_ascii_lowercase().as_str())
        {
            return Err("请勿覆盖传输层或 MCP 协议 Header。".into());
        }
    }
    Ok(url)
}
fn configured(
    mut request: reqwest::RequestBuilder,
    headers: &BTreeMap<String, String>,
) -> reqwest::RequestBuilder {
    for (key, value) in headers {
        request = request.header(key, value);
    }
    request
}
fn render(value: &Value, args: &Value) -> Result<Value, String> {
    match value {
        Value::String(text) => {
            let mut output = String::new();
            let mut remaining = text.as_str();
            while let Some(start) = remaining.find("{{") {
                output.push_str(&remaining[..start]);
                let after = &remaining[start + 2..];
                let end = after.find("}}").ok_or("工具模板缺少 }}。")?;
                let name = after[..end].trim();
                let replacement = args
                    .get(name)
                    .ok_or_else(|| format!("模板参数不存在：{name}"))?;
                if start == 0 && end + 4 == text.len() && output.is_empty() {
                    return Ok(replacement.clone());
                }
                output.push_str(
                    &replacement
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| replacement.to_string()),
                );
                remaining = &after[end + 2..];
            }
            output.push_str(remaining);
            Ok(Value::String(output))
        }
        Value::Array(array) => array
            .iter()
            .map(|v| render(v, args))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(object) => object
            .iter()
            .map(|(k, v)| render(v, args).map(|v| (k.clone(), v)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}
async fn bytes(response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "工具响应读取失败。")?;
        if output.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("工具响应超过 2 MB，已停止读取。".into());
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}
pub async fn http_tool(
    client: &reqwest::Client,
    config: &HttpTool,
    args: &Value,
) -> Result<Value, String> {
    let mut url = validate_endpoint(&config.url, &config.headers)?;
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in &config.query {
            let value = render(value, args)?;
            query.append_pair(
                key,
                &value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            );
        }
    }
    let method =
        reqwest::Method::from_bytes(config.method.as_bytes()).map_err(|_| "HTTP 工具方法无效。")?;
    let mut request = configured(client.request(method.clone(), url), &config.headers);
    if method != reqwest::Method::GET && !config.body.is_null() {
        request = request.json(&render(&config.body, args)?);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "HTTP 工具连接失败，未自动重试。")?;
    if !response.status().is_success() {
        return Err(format!(
            "HTTP 工具返回状态 {}，未自动重试。",
            response.status().as_u16()
        ));
    }
    let output = bytes(response).await?;
    let mut value = serde_json::from_slice::<Value>(&output)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&output).into_owned()));
    if !config.response_path.trim().is_empty() {
        let pointer = if config.response_path.starts_with('/') {
            config.response_path.clone()
        } else {
            format!(
                "/{}",
                config
                    .response_path
                    .split('.')
                    .map(|p| p.replace('~', "~0").replace('/', "~1"))
                    .collect::<Vec<_>>()
                    .join("/")
            )
        };
        value = value
            .pointer(&pointer)
            .cloned()
            .ok_or("工具响应路径不存在。")?;
    }
    Ok(sanitized(
        value,
        &config.url,
        &config.headers,
        Some(&config.body),
        Some(&config.query),
    ))
}

struct Session {
    session_id: Option<String>,
    version: String,
}
fn matching_rpc_event(data: &str, expected: &Value) -> Result<Option<Value>, String> {
    let value: Value = serde_json::from_str(data).map_err(|_| "MCP SSE 数据无效。")?;
    Ok((value.get("id") == Some(expected)
        && (value.get("result").is_some() || value.get("error").is_some()))
    .then_some(value))
}
fn rpc_result(value: Value, expected: &Value) -> Result<Value, String> {
    if value["jsonrpc"] != "2.0" || value.get("id") != Some(expected) {
        return Err("MCP 响应 ID 或版本不匹配。".into());
    }
    if value.get("error").is_some() {
        return Err("MCP 服务返回 JSON-RPC 错误。".into());
    }
    value
        .get("result")
        .cloned()
        .ok_or("MCP 响应缺少 result。".into())
}
pub struct McpClient {
    client: reqwest::Client,
    config: McpServer,
    session: Mutex<Option<Session>>,
}
impl McpClient {
    pub fn target(&self) -> String {
        public_target(&self.config.url)
    }
    pub fn new(client: reqwest::Client, config: McpServer) -> Self {
        Self {
            client,
            config,
            session: Mutex::new(None),
        }
    }
    async fn post(
        &self,
        message: Value,
        session: Option<&Session>,
    ) -> Result<(Value, Option<String>), String> {
        validate_endpoint(&self.config.url, &self.config.headers)?;
        let expected = message.get("id").cloned();
        let mut request = configured(
            self.client
                .post(&self.config.url)
                .header("Accept", "application/json, text/event-stream"),
            &self.config.headers,
        )
        .json(&message);
        if let Some(session) = session {
            request = request.header("MCP-Protocol-Version", &session.version);
            if let Some(id) = &session.session_id {
                request = request.header("Mcp-Session-Id", id);
            }
        }
        let response = request
            .send()
            .await
            .map_err(|_| "MCP 连接失败，未自动重试。")?;
        if !response.status().is_success() {
            return Err(format!(
                "MCP 返回 HTTP {}，请检查地址、鉴权或重新连接。",
                response.status().as_u16()
            ));
        }
        let session_id = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if expected.is_none() {
            return Ok((Value::Null, session_id));
        }
        let sse = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/event-stream"));
        let value = if sse {
            let mut decoder = SseDecoder::default();
            let mut stream = response.bytes_stream();
            let mut total = 0;
            let mut result = None;
            'reading: while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| "MCP SSE 连接中断。")?;
                total += chunk.len();
                if total > 2 * 1024 * 1024 {
                    return Err("MCP 响应超过 2 MB。".into());
                }
                for data in decoder.push(&chunk)? {
                    if let Some(value) = matching_rpc_event(&data, expected.as_ref().unwrap())? {
                        result = Some(value);
                        break 'reading;
                    }
                }
            }
            if result.is_none() {
                for data in decoder.finish()? {
                    if let Some(value) = matching_rpc_event(&data, expected.as_ref().unwrap())? {
                        result = Some(value);
                        break;
                    }
                }
            }
            result.ok_or("MCP 未返回匹配的 JSON-RPC 结果。")?
        } else {
            serde_json::from_slice(&bytes(response).await?).map_err(|_| "MCP JSON 数据无效。")?
        };
        Ok((rpc_result(value, expected.as_ref().unwrap())?, session_id))
    }
    async fn initialize(&self) -> Result<Session, String> {
        let (result,id)=self.post(json!({"jsonrpc":"2.0","id":uuid::Uuid::new_v4().to_string(),"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"oh-my-chatwise","version":"0.1.0"}}}),None).await?;
        let version = result["protocolVersion"]
            .as_str()
            .filter(|v| matches!(*v, "2024-11-05" | "2025-03-26" | "2025-06-18"))
            .ok_or("MCP 协议版本不受支持。")?
            .to_owned();
        let session = Session {
            session_id: id,
            version,
        };
        self.post(
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            Some(&session),
        )
        .await?;
        Ok(session)
    }
    pub async fn discover(&self) -> Result<Vec<McpTool>, String> {
        let session = self.initialize().await?;
        let mut cursor = None::<String>;
        let mut seen = BTreeSet::new();
        let mut tools = Vec::new();
        let mut names = BTreeSet::new();
        for _ in 0..32 {
            let mut params = json!({});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let (result,_)=self.post(json!({"jsonrpc":"2.0","id":uuid::Uuid::new_v4().to_string(),"method":"tools/list","params":params}),Some(&session)).await?;
            for value in result["tools"].as_array().ok_or("MCP 工具列表格式无效。")? {
                let name = value["name"]
                    .as_str()
                    .filter(|n| !n.is_empty() && n.len() <= 256)
                    .ok_or("MCP 工具名称无效。")?;
                if !names.insert(name.to_owned()) {
                    continue;
                }
                if value["inputSchema"]["type"] != "object" {
                    return Err("MCP 工具顶层 Schema 需要为 object。".into());
                }
                tools.push(McpTool {
                    name: name.into(),
                    description: value["description"]
                        .as_str()
                        .unwrap_or(name)
                        .chars()
                        .take(4000)
                        .collect(),
                    input_schema: value["inputSchema"].clone(),
                    enabled: false,
                    risk: Risk::ExternalAction,
                });
                if tools.len() > 64 {
                    return Err("单个 MCP 服务最多选择 64 个工具。".into());
                }
            }
            cursor = result["nextCursor"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            if let Some(cursor) = &cursor {
                if !seen.insert(cursor.clone()) {
                    return Err("MCP 分页游标重复。".into());
                }
            } else {
                *self.session.lock().await = Some(session);
                return Ok(tools);
            }
        }
        Err("MCP 分页过多。".into())
    }
    pub async fn call(
        &self,
        name: &str,
        args: &Value,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        let mut locked = self.session.lock().await;
        if locked.is_none() {
            *locked = Some(self.initialize().await?);
        }
        let session = locked.as_ref().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let message = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}});
        let (result, _) = tokio::select! {biased;_=cancel.cancelled()=>{return Err("MCP 调用已停止；远端已开始的操作可能仍会完成。".into());},result=self.post(message,Some(session))=>result?};
        let result = sanitized(result, &self.config.url, &self.config.headers, None, None);
        if result["isError"] == true {
            return Err(result["content"]
                .as_array()
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_else(|| "MCP 工具执行失败。".into()));
        }
        Ok(result)
    }
}
pub async fn discover(client: reqwest::Client, server: McpServer) -> Result<Vec<McpTool>, String> {
    tokio::time::timeout(
        Duration::from_secs(30),
        McpClient::new(client, server).discover(),
    )
    .await
    .map_err(|_| "MCP 工具发现超时。".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_rpc_matches_ids_ignores_pushes_and_decodes_fragmented_sse() {
        let id = json!("request");
        for message in [
            json!({"jsonrpc":"2.0","method":"notifications/progress"}),
            json!({"jsonrpc":"2.0","id":"server-request","method":"sampling/createMessage"}),
            json!({"jsonrpc":"2.0","id":"other","result":{}}),
        ] {
            assert!(
                matching_rpc_event(&message.to_string(), &id)
                    .unwrap()
                    .is_none()
            );
        }
        let raw = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":\"request\",\"result\":{\"text\":\"中文📷\"}}";
        let mut decoder = SseDecoder::default();
        let mut records = Vec::new();
        for byte in raw.as_bytes() {
            records.extend(decoder.push(&[*byte]).unwrap());
        }
        records.extend(decoder.finish().unwrap());
        let response = matching_rpc_event(&records[0], &id).unwrap().unwrap();
        assert_eq!(rpc_result(response, &id).unwrap()["text"], "中文📷");
        assert!(
            rpc_result(
                json!({"jsonrpc":"2.0","id":"request","error":{"code":-1}}),
                &id
            )
            .is_err()
        );
        assert!(rpc_result(json!({"jsonrpc":"1.0","id":"request","result":{}}), &id).is_err());
        assert!(rpc_result(json!({"jsonrpc":"2.0","id":"wrong","result":{}}), &id).is_err());
    }
    #[test]
    fn output_redaction_preserves_json_and_hides_static_authentication() {
        let headers = BTreeMap::from([("Authorization".into(), "Bearer header-secret".into())]);
        let body = json!({"nested":{"api_key":"body-secret"},"query":"{{query}}"});
        let query = BTreeMap::from([("token".into(), json!("query-secret"))]);
        let value = json!({"text":"header-secret body-secret query-secret url-secret", "items":["Bearer header-secret", 4]});
        let url = "https://search.example/query?api_key=url-secret";
        let cleaned = sanitized(value, url, &headers, Some(&body), Some(&query));
        let text = cleaned.to_string();
        for secret in ["header-secret", "body-secret", "query-secret", "url-secret"] {
            assert!(!text.contains(secret));
        }
        assert_eq!(cleaned["items"][1], 4);
        assert_eq!(public_target(url), "https://search.example/query");
    }
    #[test]
    fn template_substitution_preserves_json_types_and_does_not_recurse() {
        let args = json!({"query":"hello {{secret}}","limit":3,"nested":{"ok":true}});
        assert_eq!(render(&json!({"q":"{{query}}","limit":"{{limit}}","nested":"{{nested}}","prefix":"q: {{query}}"}),&args).unwrap(),json!({"q":"hello {{secret}}","limit":3,"nested":{"ok":true},"prefix":"q: hello {{secret}}"}));
        assert!(render(&json!("{{missing}}"), &args).is_err());
    }
    #[test]
    fn endpoint_validation_prevents_transport_header_override() {
        assert!(
            validate_endpoint(
                "https://server.example/mcp",
                &BTreeMap::from([("Mcp-Session-Id".into(), "x".into())])
            )
            .is_err()
        );
        assert!(validate_endpoint("file:///tmp/tool", &BTreeMap::new()).is_err());
        assert!(validate_endpoint("http://127.0.0.1:1234/mcp", &BTreeMap::new()).is_ok());
    }
}
