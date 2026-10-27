use crate::{
    llm,
    models::{AvailableModel, Connection, Protocol},
};
use futures_util::StreamExt;
use reqwest::{Client, Url};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub fn endpoint(connection: &Connection) -> Result<Url, String> {
    let mut url = llm::endpoint(&connection.base_url, &connection.protocol)?;
    let suffix = match connection.protocol {
        Protocol::ChatCompletions => "/chat/completions",
        Protocol::Responses => "/responses",
        Protocol::Anthropic => "/messages",
    };
    let prefix = url
        .path()
        .strip_suffix(suffix)
        .ok_or("无法确定模型列表地址。")?
        .to_owned();
    url.set_path(&format!("{prefix}/models"));
    Ok(url)
}

fn parse_page(value: &Value) -> Result<Vec<AvailableModel>, String> {
    let data = value["data"]
        .as_array()
        .ok_or("模型列表格式无效，服务应返回包含 data 数组的 JSON。")?;
    let mut models = Vec::new();
    for item in data {
        if let Some(id) = item["id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            models.push(AvailableModel {
                id: id.to_owned(),
                name: item["display_name"]
                    .as_str()
                    .or_else(|| item["name"].as_str())
                    .map(str::trim)
                    .filter(|name| !name.is_empty() && *name != id)
                    .map(str::to_owned),
            });
        }
    }
    if !data.is_empty() && models.is_empty() {
        return Err("模型列表中没有有效的模型 ID。".into());
    }
    Ok(models)
}

pub async fn fetch(
    client: &Client,
    connection: &Connection,
    key: Option<&str>,
) -> Result<Vec<AvailableModel>, String> {
    let base = endpoint(connection)?;
    let mut cursor: Option<String> = None;
    let mut visited = HashSet::new();
    let mut models = BTreeMap::new();
    for _ in 0..100 {
        let mut url = base.clone();
        if connection.protocol == Protocol::Anthropic {
            url.query_pairs_mut().append_pair("limit", "100");
        }
        if let Some(cursor) = &cursor {
            let name = if connection.protocol == Protocol::Anthropic {
                "after_id"
            } else {
                "after"
            };
            url.query_pairs_mut().append_pair(name, cursor);
        }
        let mut request = client.get(url).header("Accept", "application/json");
        if connection.protocol == Protocol::Anthropic {
            request = request.header("anthropic-version", "2023-06-01");
            if let Some(key) = key {
                request = request.header("x-api-key", key);
            }
        } else if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                "获取模型列表超时，请检查网络后重试。"
            } else {
                "无法连接模型列表接口，请检查 API 地址和网络。"
            }
            .to_string()
        })?;
        let status = response.status();
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "读取模型列表失败，请重试。".to_string())?;
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("模型列表响应超过大小限制。".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        // Never reflect raw upstream response bodies: they may contain credentials or HTML.
        if !status.is_success() {
            let hint = match status.as_u16() {
                401 | 403 => "认证失败或无权列出模型，请检查 API 密钥与权限。",
                404 | 405 => "服务未提供模型列表接口，请确认 API 地址或联系服务方。",
                429 => "模型列表请求受限，请稍后重试。",
                _ => "获取模型列表失败，请检查服务状态后重试。",
            };
            return Err(format!("HTTP {} · {hint}", status.as_u16()));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "模型列表接口未返回有效 JSON，请检查 API 地址。".to_string())?;
        if value.get("error").is_some_and(|error| !error.is_null()) {
            return Err("模型列表接口返回错误，请检查 API 密钥与服务状态。".into());
        }
        let page = parse_page(&value)?;
        let last = page.last().map(|model| model.id.clone());
        for model in page {
            models.entry(model.id.clone()).or_insert(model);
        }
        if models.len() > 10_000 {
            return Err("模型数量超过 10000 个，无法加载完整列表。".into());
        }
        if value["has_more"] != true {
            return Ok(models.into_values().collect());
        }
        let next = value["last_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .or(last)
            .ok_or("模型列表分页缺少游标，无法读取下一页。")?;
        if !visited.insert(next.clone()) {
            return Err("服务返回了重复的分页游标，已停止获取模型。".into());
        }
        cursor = Some(next);
    }
    Err("模型列表分页超过限制，无法读取完整列表。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    fn connection(protocol: Protocol, base_url: &str) -> Connection {
        Connection {
            id: "test".into(),
            name: "test".into(),
            protocol,
            base_url: base_url.into(),
            models: vec![],
            model_names: Default::default(),
            model_reasoning: Default::default(),
            has_api_key: false,
        }
    }
    #[test]
    fn derives_models_url_from_base_and_full_endpoints() {
        for (protocol, input) in [
            (
                Protocol::ChatCompletions,
                "https://example.test/proxy/v1/chat/completions/",
            ),
            (
                Protocol::Responses,
                "https://example.test/proxy/v1/responses",
            ),
            (Protocol::Anthropic, "https://example.test/proxy/v1/"),
        ] {
            assert_eq!(
                endpoint(&connection(protocol, input)).unwrap().as_str(),
                "https://example.test/proxy/v1/models"
            );
        }
        assert!(endpoint(&connection(Protocol::Responses, "file:///secret")).is_err());
    }
    #[tokio::test]
    async fn verifies_all_protocols_auth_pagination_and_deduplication() {
        for protocol in [
            Protocol::ChatCompletions,
            Protocol::Responses,
            Protocol::Anthropic,
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = format!("http://{}/proxy/v1", listener.local_addr().unwrap());
            let expected = protocol.clone();
            let server = tokio::spawn(async move {
                for page in 0..2 {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut bytes = Vec::new();
                    let mut buffer = [0; 2048];
                    while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                        let read = socket.read(&mut buffer).await.unwrap();
                        assert!(read > 0);
                        bytes.extend_from_slice(&buffer[..read]);
                    }
                    let headers = String::from_utf8_lossy(&bytes).to_lowercase();
                    assert!(headers.starts_with("get /proxy/v1/models"));
                    if expected == Protocol::Anthropic {
                        assert!(headers.contains("x-api-key: draft-secret"));
                        assert!(headers.contains("anthropic-version: 2023-06-01"));
                        assert!(headers.contains("limit=100"));
                        if page == 1 {
                            assert!(headers.contains("after_id=model-b"));
                        }
                    } else {
                        assert!(headers.contains("authorization: bearer draft-secret"));
                        if page == 1 {
                            assert!(headers.contains("after=model-b"));
                        }
                    }
                    let body = if page == 0 {
                        r#"{"data":[{"id":"model-b","display_name":"模型 B"},{"id":"model-a"}],"has_more":true,"last_id":"model-b"}"#
                    } else {
                        r#"{"data":[{"id":"model-a"},{"id":"model-c"}],"has_more":false}"#
                    };
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                }
            });
            let result = fetch(
                &llm::client().unwrap(),
                &connection(protocol, &address),
                Some("draft-secret"),
            )
            .await
            .unwrap();
            server.await.unwrap();
            assert_eq!(
                result
                    .iter()
                    .map(|model| model.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["model-a", "model-b", "model-c"]
            );
            assert_eq!(result[1].name.as_deref(), Some("模型 B"));
        }
    }
    #[tokio::test]
    async fn reports_auth_and_missing_endpoint_errors_without_exposing_response_body() {
        for status in [401, 404] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = format!("http://{}/v1", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 2048];
                let read = socket.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                let body = r#"{"error":{"message":"echoed-secret"}}"#;
                socket.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            let error = fetch(
                &llm::client().unwrap(),
                &connection(Protocol::Responses, &address),
                None,
            )
            .await
            .unwrap_err();
            server.await.unwrap();
            assert!(error.contains(&status.to_string()));
            assert!(!error.contains("echoed-secret"));
        }
    }
    #[test]
    fn validates_list_shape_and_keeps_empty_lists_valid() {
        assert!(parse_page(&serde_json::json!({"models":[]})).is_err());
        assert!(parse_page(&serde_json::json!({"data":[{"id":""}]})).is_err());
        assert!(
            parse_page(&serde_json::json!({"data":[]}))
                .unwrap()
                .is_empty()
        );
    }
}
