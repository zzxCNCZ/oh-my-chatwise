use crate::{
    models::{ChatRequest, Protocol, StreamEvent},
    sse::SseDecoder,
};
use futures_util::StreamExt;
use reqwest::{Client, Url};
use serde_json::{Value, json};
use std::time::Duration;

pub fn endpoint(base: &str, protocol: &Protocol) -> Result<Url, String> {
    let mut url = Url::parse(base.trim()).map_err(|_| "API 地址无效。".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("API 地址必须为 HTTP(S) 地址，且不能包含查询参数或账号密码。".into());
    }
    let suffix = match protocol {
        Protocol::ChatCompletions => "/chat/completions",
        Protocol::Responses => "/responses",
        Protocol::Anthropic => "/messages",
    };
    let path = url.path().trim_end_matches('/').to_owned();
    if !path.ends_with(suffix) {
        if ["/chat/completions", "/responses", "/messages"]
            .iter()
            .any(|other| path.ends_with(other))
        {
            return Err("API 端点与所选协议不匹配。".into());
        }
        url.set_path(&format!("{path}{suffix}"));
    } else {
        url.set_path(&path);
    }
    Ok(url)
}
fn message_payload(
    message: &crate::models::ChatMessage,
    protocol: &Protocol,
) -> Result<Value, String> {
    let mut content = message.content.clone();
    for attachment in &message.text_attachments {
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&attachment.context);
    }
    if message.images.is_empty() {
        return Ok(json!({"role":message.role,"content":content}));
    }
    if message.role != "user" {
        return Err("图片仅支持用户消息。".into());
    }
    let mut parts = Vec::new();
    if !content.trim().is_empty() {
        parts.push(match protocol {
            Protocol::Responses => json!({"type":"input_text","text":content}),
            _ => json!({"type":"text","text":content}),
        });
    }
    for image in &message.images {
        let uri = match image.source.as_str() {
            "base64" => {
                if !matches!(
                    image.media_type.as_str(),
                    "image/png" | "image/jpeg" | "image/webp"
                ) {
                    return Err("不支持的图片格式。".into());
                }
                format!(
                    "data:{};base64,{}",
                    image.media_type,
                    image.data.as_deref().ok_or("图片数据未加载。")?
                )
            }
            "url" => {
                let url = image.url.as_deref().ok_or("图片 URL 为空。")?;
                crate::attachments::validate_url(url)?;
                url.to_owned()
            }
            _ => return Err("无效的图片输入类型。".into()),
        };
        parts.push(match protocol {
            Protocol::ChatCompletions => json!({"type":"image_url","image_url":{"url":uri}}),
            Protocol::Responses => json!({"type":"input_image","image_url":uri}),
            Protocol::Anthropic => {
                let source = if image.source == "base64" {
                    json!({"type":"base64","media_type":image.media_type,"data":image.data})
                } else {
                    json!({"type":"url","url":uri})
                };
                json!({"type":"image","source":source})
            }
        });
    }
    Ok(json!({"role":message.role,"content":parts}))
}
pub fn payload(request: &ChatRequest) -> Result<Value, String> {
    if request.model.trim().is_empty() || request.messages.is_empty() {
        return Err("请选择模型并输入消息。".into());
    }
    if !(1..=128000).contains(&request.max_tokens) {
        return Err("最大输出 Tokens 必须在 1 到 128000 之间。".into());
    }
    if request
        .messages
        .iter()
        .any(|message| !matches!(message.role.as_str(), "user" | "assistant"))
    {
        return Err("不支持的消息角色。".into());
    }
    let effort = request
        .connection
        .model_reasoning
        .get(&request.model)
        .cloned()
        .unwrap_or_default();
    use crate::models::ReasoningEffort;
    if let Some(temperature) = request
        .temperature
        .filter(|_| matches!(effort, ReasoningEffort::Off | ReasoningEffort::Default))
    {
        let max = if request.connection.protocol == Protocol::Anthropic {
            1.0
        } else {
            2.0
        };
        if !temperature.is_finite() || !(0.0..=max).contains(&temperature) {
            return Err(format!("Temperature 必须在 0 到 {max} 之间。"));
        }
    }
    let mut body = json!({ "model": request.model, "stream": true });
    match request.connection.protocol {
        Protocol::ChatCompletions => {
            let mut messages = Vec::new();
            if !request.system_prompt.trim().is_empty() {
                messages.push(json!({"role":"system", "content":request.system_prompt}));
            }
            for message in &request.messages {
                messages.push(message_payload(message, &request.connection.protocol)?);
            }
            body["messages"] = json!(messages);
            body["max_completion_tokens"] = json!(request.max_tokens);
            body["stream_options"] = json!({"include_usage":true});
        }
        Protocol::Responses => {
            body["input"] = Value::Array(
                request
                    .messages
                    .iter()
                    .map(|message| message_payload(message, &request.connection.protocol))
                    .collect::<Result<_, _>>()?,
            );
            body["max_output_tokens"] = json!(request.max_tokens);
            body["store"] = json!(false);
            if !request.system_prompt.trim().is_empty() {
                body["instructions"] = json!(request.system_prompt);
            }
        }
        Protocol::Anthropic => {
            body["messages"] = Value::Array(
                request
                    .messages
                    .iter()
                    .map(|message| message_payload(message, &request.connection.protocol))
                    .collect::<Result<_, _>>()?,
            );
            body["max_tokens"] = json!(request.max_tokens);
            if !request.system_prompt.trim().is_empty() {
                body["system"] = json!(request.system_prompt);
            }
        }
    }

    let value = serde_json::to_value(&effort).unwrap();
    match (&request.connection.protocol, &effort) {
        (_, ReasoningEffort::Default) => {}
        (Protocol::ChatCompletions, _) => {
            body["reasoning_effort"] = if effort == ReasoningEffort::Off {
                json!("none")
            } else {
                value.clone()
            }
        }
        (Protocol::Responses, _) => {
            body["reasoning"] = json!({"effort": if effort == ReasoningEffort::Off { json!("none") } else { value.clone() }})
        }
        (Protocol::Anthropic, ReasoningEffort::Off) => {
            body["thinking"] = json!({"type":"disabled"})
        }
        (Protocol::Anthropic, ReasoningEffort::Minimal) => {
            return Err(
                "Anthropic 不支持 minimal 思考强度，请在模型配置中选择 low 或其他强度。".into(),
            );
        }
        (Protocol::Anthropic, _) => {
            body["thinking"] = json!({"type":"adaptive"});
            body["output_config"] = json!({"effort":value});
        }
    }
    // Reasoning APIs commonly reject sampling controls while thinking is enabled.
    if let Some(temperature) = request
        .temperature
        .filter(|_| matches!(effort, ReasoningEffort::Off | ReasoningEffort::Default))
    {
        body["temperature"] = json!(temperature);
    }
    Ok(body)
}
pub fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建网络客户端。".into())
}
#[derive(Default, Debug)]
pub struct Decoded {
    pub events: Vec<StreamEvent>,
    pub terminal: bool,
    pub finished: bool,
}
fn text(events: &mut Vec<StreamEvent>, value: Option<&str>, reasoning: bool) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        events.push(if reasoning {
            StreamEvent::Reasoning {
                text: value.to_owned(),
            }
        } else {
            StreamEvent::Text {
                text: value.to_owned(),
            }
        });
    }
}
fn usage(events: &mut Vec<StreamEvent>, value: &Value, chat: bool) {
    let input = value[if chat {
        "prompt_tokens"
    } else {
        "input_tokens"
    }]
    .as_u64();
    let output = value[if chat {
        "completion_tokens"
    } else {
        "output_tokens"
    }]
    .as_u64();
    if input.is_some() || output.is_some() {
        events.push(StreamEvent::Usage {
            input_tokens: input,
            output_tokens: output,
        });
    }
}
fn api_error(value: &Value) -> Option<String> {
    let error = value.get("error").filter(|value| !value.is_null())?;
    Some(
        error["message"]
            .as_str()
            .or_else(|| error.as_str())
            .unwrap_or("服务返回错误，请检查连接配置。")
            .chars()
            .take(600)
            .collect(),
    )
}
pub fn decode(protocol: &Protocol, data: &str) -> Result<Decoded, String> {
    if data.trim() == "[DONE]" {
        return Ok(Decoded {
            terminal: true,
            ..Default::default()
        });
    }
    let value: Value = serde_json::from_str(data)
        .map_err(|_| "服务返回了无法解析的流式数据，请检查所选 API 协议。".to_string())?;
    if let Some(error) = api_error(&value) {
        return Err(error);
    }
    let mut result = Decoded::default();
    match protocol {
        Protocol::ChatCompletions => {
            let choice = &value["choices"][0];
            text(
                &mut result.events,
                choice["delta"]["content"].as_str(),
                false,
            );
            text(
                &mut result.events,
                choice["delta"]["reasoning_content"].as_str(),
                true,
            );
            usage(&mut result.events, &value["usage"], true);
            if let Some(reason) = choice["finish_reason"].as_str() {
                if reason == "length" {
                    return Err(
                        "已达到最大输出 Tokens，回复可能不完整。可提高上限后重新生成。".into(),
                    );
                }
                if reason == "content_filter" {
                    return Err("回复被服务的内容过滤器中断。".into());
                }
                result.finished = true;
            }
        }
        Protocol::Responses => match value["type"].as_str().unwrap_or("") {
            "response.output_text.delta" | "response.refusal.delta" => {
                text(&mut result.events, value["delta"].as_str(), false)
            }
            "response.reasoning_summary_text.delta" => {
                text(&mut result.events, value["delta"].as_str(), true)
            }
            "response.completed" => {
                usage(&mut result.events, &value["response"]["usage"], false);
                result.terminal = true;
            }
            "response.failed" => {
                return Err(
                    api_error(&value["response"]).unwrap_or("模型生成失败，请重试。".into())
                );
            }
            "response.incomplete" => {
                return Err("回复未完成，可能已达到输出上限。请检查模型参数后重试。".into());
            }
            "error" => {
                return Err(value["message"]
                    .as_str()
                    .unwrap_or("Responses API 返回错误。")
                    .chars()
                    .take(600)
                    .collect());
            }
            _ => {}
        },
        Protocol::Anthropic => match value["type"].as_str().unwrap_or("") {
            "content_block_start" => text(
                &mut result.events,
                value["content_block"]["text"].as_str(),
                false,
            ),
            "content_block_delta" => {
                text(&mut result.events, value["delta"]["text"].as_str(), false);
                text(
                    &mut result.events,
                    value["delta"]["thinking"].as_str(),
                    true,
                );
            }
            "message_start" => usage(&mut result.events, &value["message"]["usage"], false),
            "message_delta" => {
                usage(&mut result.events, &value["usage"], false);
                if value["delta"]["stop_reason"] == "max_tokens" {
                    return Err(
                        "已达到最大输出 Tokens，回复可能不完整。可提高上限后重新生成。".into(),
                    );
                }
            }
            "message_stop" => result.terminal = true,
            _ => {}
        },
    }
    Ok(result)
}
pub(crate) fn decode_json(protocol: &Protocol, value: &Value) -> Result<Vec<StreamEvent>, String> {
    if let Some(error) = api_error(value) {
        return Err(error);
    }
    let mut events = Vec::new();
    match protocol {
        Protocol::ChatCompletions => {
            text(
                &mut events,
                value["choices"][0]["message"]["content"].as_str(),
                false,
            );
            usage(&mut events, &value["usage"], true);
            if value["choices"][0]["finish_reason"] == "length" {
                return Err("服务返回了截断的回复，请提高输出上限后重试。".into());
            }
        }
        Protocol::Responses => {
            if value["status"] == "failed" || value["status"] == "incomplete" {
                return Err("服务返回了未完成的回复，请检查输出上限后重试。".into());
            }
            if let Some(output) = value["output"].as_array() {
                for item in output {
                    if let Some(content) = item["content"].as_array() {
                        for part in content {
                            text(
                                &mut events,
                                part["text"].as_str().or(part["refusal"].as_str()),
                                false,
                            );
                        }
                    }
                }
            }
            usage(&mut events, &value["usage"], false);
        }
        Protocol::Anthropic => {
            if let Some(content) = value["content"].as_array() {
                for part in content {
                    text(&mut events, part["text"].as_str(), false);
                }
            }
            usage(&mut events, &value["usage"], false);
        }
    }
    Ok(events)
}
pub async fn execute(
    client: &Client,
    request: &ChatRequest,
    key: Option<&str>,
    mut emit: impl FnMut(StreamEvent) -> Result<(), String>,
) -> Result<(), String> {
    let url = endpoint(&request.connection.base_url, &request.connection.protocol)?;
    let body = payload(request)?;
    let mut http = client
        .post(url)
        .header("Accept", "text/event-stream")
        .json(&body);
    if request.connection.protocol == Protocol::Anthropic {
        http = http.header("anthropic-version", "2023-06-01");
        if let Some(key) = key {
            http = http.header("x-api-key", key);
        }
    } else if let Some(key) = key {
        http = http.bearer_auth(key);
    }
    let response = http.send().await.map_err(|error| {
        if error.is_timeout() {
            "连接超时，请检查 API 地址和网络。"
        } else {
            "无法连接到 API，请检查地址、网络和系统代理。"
        }
        .to_string()
    })?;
    let status = response.status();
    let is_json = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"));
    let mut stream = response.bytes_stream();
    if !status.is_success() || is_json {
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "读取服务响应失败。".to_string())?;
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("服务响应过大。".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            let hint = match status.as_u16() {
                401 | 403 => "认证失败，请检查 API 密钥与模型权限。",
                404 => "接口不存在，请检查基础地址、协议和模型 ID。",
                429 => "请求受限或额度不足，请稍后重试。",
                _ => "服务请求失败，请检查配置或稍后重试。",
            };
            return Err(format!(
                "HTTP {} · {}{}",
                status.as_u16(),
                hint,
                api_error(&value)
                    .map(|error| format!("\n{error}"))
                    .unwrap_or_default()
            ));
        }
        for event in decode_json(&request.connection.protocol, &value)? {
            emit(event)?;
        }
        return Ok(());
    }
    let mut decoder = SseDecoder::default();
    let mut finished = false;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|_| "流式连接已中断。已接收的内容已保留，可重试。".to_string())?;
        for data in decoder.push(&chunk)? {
            let decoded = decode(&request.connection.protocol, &data)?;
            for event in decoded.events {
                emit(event)?;
            }
            finished |= decoded.finished;
            if decoded.terminal {
                return Ok(());
            }
        }
    }
    for data in decoder.finish()? {
        let decoded = decode(&request.connection.protocol, &data)?;
        for event in decoded.events {
            emit(event)?;
        }
        finished |= decoded.finished || decoded.terminal;
    }
    if finished {
        Ok(())
    } else {
        Err("服务在回复完成前关闭了连接。已接收的内容已保留，请重试。".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChatMessage, Connection};
    fn request(protocol: Protocol) -> ChatRequest {
        ChatRequest {
            id: "test".into(),
            connection: Connection {
                id: "test".into(),
                name: "local".into(),
                protocol,
                base_url: "http://localhost:1234/v1".into(),
                models: vec!["custom-model".into()],
                model_names: Default::default(),
                model_reasoning: Default::default(),
                has_api_key: false,
            },
            model: "custom-model".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "你好".into(),
                images: vec![],
                text_attachments: vec![],
                agent_steps: vec![],
            }],
            tools: Default::default(),
            system_prompt: "请用中文回答".into(),
            max_tokens: 4096,
            temperature: None,
            agent: Default::default(),
        }
    }
    #[test]
    fn adapts_image_inputs_and_mixed_text_for_each_protocol() {
        for protocol in [
            Protocol::ChatCompletions,
            Protocol::Responses,
            Protocol::Anthropic,
        ] {
            let mut request = request(protocol.clone());
            request.messages[0].images = vec![
                crate::models::ImageAttachment {
                    source: "base64".into(),
                    media_type: "image/png".into(),
                    data: Some(crate::attachments::TEST_PNG.into()),
                    ..Default::default()
                },
                crate::models::ImageAttachment {
                    source: "url".into(),
                    url: Some("https://cdn.test/photo.png".into()),
                    ..Default::default()
                },
            ];
            let value = payload(&request).unwrap();
            let content = match protocol {
                Protocol::Responses => &value["input"][0]["content"],
                Protocol::ChatCompletions => &value["messages"][1]["content"],
                Protocol::Anthropic => &value["messages"][0]["content"],
            };
            assert_eq!(content[0]["text"], "你好");
            match protocol {
                Protocol::ChatCompletions => {
                    assert!(
                        content[1]["image_url"]["url"]
                            .as_str()
                            .unwrap()
                            .starts_with("data:image/png;base64,")
                    );
                    assert_eq!(content[2]["image_url"]["url"], "https://cdn.test/photo.png");
                }
                Protocol::Responses => {
                    assert_eq!(content[1]["type"], "input_image");
                    assert!(
                        content[1]["image_url"]
                            .as_str()
                            .unwrap()
                            .starts_with("data:image/png;base64,")
                    );
                }
                Protocol::Anthropic => {
                    assert_eq!(content[1]["source"]["type"], "base64");
                    assert_eq!(content[1]["source"]["data"], crate::attachments::TEST_PNG);
                    assert_eq!(content[2]["source"]["type"], "url");
                }
            }
            request.messages[0].content.clear();
            assert!(payload(&request).is_ok());
        }
    }
    #[test]
    fn endpoint_preserves_custom_prefix_and_avoids_duplicate_paths() {
        assert_eq!(
            endpoint("https://local.example/proxy/v1/", &Protocol::Responses)
                .unwrap()
                .as_str(),
            "https://local.example/proxy/v1/responses"
        );
        assert_eq!(
            endpoint("https://local.example/v1/messages/", &Protocol::Anthropic)
                .unwrap()
                .as_str(),
            "https://local.example/v1/messages"
        );
        assert!(endpoint("https://local.example/v1/messages", &Protocol::Responses).is_err());
        assert!(endpoint("file:///etc/passwd", &Protocol::Responses).is_err());
        assert!(endpoint("https://local.example?key=secret", &Protocol::Responses).is_err());
    }
    #[test]
    fn reasoning_defaults_medium_and_adapts_off_and_compatibility_per_protocol() {
        use crate::models::ReasoningEffort;
        for protocol in [
            Protocol::ChatCompletions,
            Protocol::Responses,
            Protocol::Anthropic,
        ] {
            let mut req = request(protocol.clone());
            req.temperature = Some(0.7);
            let body = payload(&req).unwrap();
            match protocol {
                Protocol::ChatCompletions => assert_eq!(body["reasoning_effort"], "medium"),
                Protocol::Responses => assert_eq!(body["reasoning"]["effort"], "medium"),
                Protocol::Anthropic => {
                    assert_eq!(body["thinking"]["type"], "adaptive");
                    assert_eq!(body["output_config"]["effort"], "medium");
                }
            }
            assert!(body.get("temperature").is_none());
            for effort in [
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
                ReasoningEffort::Max,
            ] {
                req.connection
                    .model_reasoning
                    .insert(req.model.clone(), effort.clone());
                let body = payload(&req).unwrap();
                let expected = serde_json::to_value(effort).unwrap();
                let actual = match protocol {
                    Protocol::ChatCompletions => &body["reasoning_effort"],
                    Protocol::Responses => &body["reasoning"]["effort"],
                    Protocol::Anthropic => &body["output_config"]["effort"],
                };
                assert_eq!(actual, &expected);
            }
            req.connection
                .model_reasoning
                .insert(req.model.clone(), ReasoningEffort::Off);
            let body = payload(&req).unwrap();
            match protocol {
                Protocol::ChatCompletions => assert_eq!(body["reasoning_effort"], "none"),
                Protocol::Responses => assert_eq!(body["reasoning"]["effort"], "none"),
                Protocol::Anthropic => {
                    assert_eq!(body["thinking"]["type"], "disabled");
                    assert!(body.get("output_config").is_none());
                }
            }
            assert_eq!(body["temperature"], 0.7);
            req.connection
                .model_reasoning
                .insert(req.model.clone(), ReasoningEffort::Default);
            let body = payload(&req).unwrap();
            for key in ["reasoning_effort", "reasoning", "thinking", "output_config"] {
                assert!(body.get(key).is_none());
            }
            assert_eq!(body["temperature"], 0.7);
        }
        let mut req = request(Protocol::Anthropic);
        req.connection
            .model_reasoning
            .insert(req.model.clone(), ReasoningEffort::Minimal);
        assert!(payload(&req).is_err());
    }
    #[test]
    fn protocols_use_distinct_system_and_token_parameters() {
        let chat = payload(&request(Protocol::ChatCompletions)).unwrap();
        assert_eq!(chat["messages"][0]["role"], "system");
        assert_eq!(chat["max_completion_tokens"], 4096);
        assert!(chat.get("temperature").is_none());
        let responses = payload(&request(Protocol::Responses)).unwrap();
        assert_eq!(responses["instructions"], "请用中文回答");
        assert_eq!(responses["input"][0]["role"], "user");
        assert_eq!(responses["store"], false);
        let anthropic = payload(&request(Protocol::Anthropic)).unwrap();
        assert_eq!(anthropic["system"], "请用中文回答");
        assert_eq!(anthropic["max_tokens"], 4096);
        assert_eq!(anthropic["messages"][0]["role"], "user");
    }
    #[test]
    fn decodes_all_three_protocols_and_usage() {
        let chat = decode(
            &Protocol::ChatCompletions,
            r#"{"choices":[{"delta":{"content":"你好"}}]}"#,
        )
        .unwrap();
        let responses = decode(
            &Protocol::Responses,
            r#"{"type":"response.output_text.delta","delta":"你好"}"#,
        )
        .unwrap();
        let anthropic = decode(
            &Protocol::Anthropic,
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"你好"}}"#,
        )
        .unwrap();
        assert_eq!(chat.events, responses.events);
        assert_eq!(responses.events, anthropic.events);
        let usage = decode(
            &Protocol::Anthropic,
            r#"{"type":"message_delta","usage":{"output_tokens":12}}"#,
        )
        .unwrap();
        assert_eq!(
            usage.events,
            vec![StreamEvent::Usage {
                input_tokens: None,
                output_tokens: Some(12)
            }]
        );
        assert!(decode(&Protocol::Responses, r#"{"type":"response.completed","response":{"usage":{"input_tokens":2,"output_tokens":3}}}"#).unwrap().terminal);
        assert!(
            decode(&Protocol::Anthropic, r#"{"type":"message_stop"}"#)
                .unwrap()
                .terminal
        );
        assert!(
            decode(&Protocol::ChatCompletions, "[DONE]")
                .unwrap()
                .terminal
        );
    }
    #[test]
    fn exposes_stream_errors_instead_of_silent_success() {
        assert!(
            decode(
                &Protocol::Anthropic,
                r#"{"type":"error","error":{"message":"overloaded"}}"#
            )
            .unwrap_err()
            .contains("overloaded")
        );
        assert!(
            decode(
                &Protocol::Responses,
                r#"{"type":"response.failed","response":{"error":{"message":"failed"}}}"#
            )
            .is_err()
        );
        assert!(decode(&Protocol::ChatCompletions, "not json").is_err());
    }
    #[tokio::test]
    async fn local_http_server_verifies_auth_payload_and_stream_for_each_protocol() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        for protocol in [
            Protocol::ChatCompletions,
            Protocol::Responses,
            Protocol::Anthropic,
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let mut request = request(protocol.clone());
            request.connection.base_url = format!("http://{address}/v1");
            let expected_protocol = protocol.clone();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n")
                    {
                        let headers = String::from_utf8_lossy(&bytes[..header_end]).to_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length:")
                                    .map(|length| length.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= header_end + 4 + length {
                            let body: Value =
                                serde_json::from_slice(&bytes[header_end + 4..]).unwrap();
                            assert_eq!(body["model"], "custom-model");
                            if expected_protocol == Protocol::Anthropic {
                                assert!(headers.contains("x-api-key: test-secret"));
                                assert!(headers.contains("anthropic-version: 2023-06-01"));
                            } else {
                                assert!(headers.contains("authorization: bearer test-secret"));
                            }
                            break;
                        }
                    }
                }
                let body = match expected_protocol {
                    Protocol::ChatCompletions => {
                        "data: {\"choices\":[{\"delta\":{\"content\":\"你好 🌱\"}}]}\n\ndata: [DONE]\n\n"
                    }
                    Protocol::Responses => {
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"你好 🌱\"}\n\ndata: {\"type\":\"response.completed\"}\n\n"
                    }
                    Protocol::Anthropic => {
                        "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"你好 🌱\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n"
                    }
                };
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                // Deliberately split at byte boundaries, including inside Chinese characters.
                for byte in body.as_bytes() {
                    socket.write_all(&[*byte]).await.unwrap();
                }
            });
            let mut events = Vec::new();
            execute(&client().unwrap(), &request, Some("test-secret"), |event| {
                events.push(event);
                Ok(())
            })
            .await
            .unwrap();
            server.await.unwrap();
            assert_eq!(
                events,
                vec![StreamEvent::Text {
                    text: "你好 🌱".into()
                }]
            );
        }
    }
}
