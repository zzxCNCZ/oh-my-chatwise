use crate::{attachments, models::ImageUploadConfig};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use reqwest::{Client, Method, multipart};
use serde_json::Value;

pub const CONFIG_KEY: &str = "image-upload-config";
pub fn validate(config: &ImageUploadConfig) -> Result<(), String> {
    let url = render_url(&config.url, "test.png", "test")?;
    attachments::validate_url(&url)?;
    if !matches!(config.method.as_str(), "POST" | "PUT" | "PATCH") {
        return Err("上传方法仅支持 POST、PUT、PATCH。".into());
    }
    if !matches!(
        config.body_type.as_str(),
        "multipart" | "json" | "form-urlencoded" | "binary"
    ) {
        return Err("不支持的上传 Body 类型。".into());
    }
    if !config.body.is_object() {
        return Err("Body 参数必须是 JSON 对象。".into());
    }
    if !matches!(
        config.response_type.as_str(),
        "json" | "text" | "url-template"
    ) {
        return Err("不支持的 URL 提取方式。".into());
    }
    if config.response_type == "json" && config.response_url_path.trim().is_empty() {
        return Err("请填写响应 URL 的字段路径。".into());
    }
    if config.response_type == "url-template" {
        attachments::validate_url(&render_url(
            &config.public_url_template,
            "test.png",
            "test",
        )?)?;
    }
    if config.body_type == "multipart" && config.file_field.trim().is_empty() {
        return Err("请填写 multipart 文件字段名称。".into());
    }
    for (name, value) in &config.headers {
        reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| "Header 名称无效。".to_string())?;
        reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| "Header 值无效，不能包含换行。".to_string())?;
        if config.body_type == "multipart" && name.eq_ignore_ascii_case("content-type") {
            return Err(
                "multipart 的 Content-Type 与 boundary 会自动生成，请移除此 Header。".into(),
            );
        }
    }
    if serde_json::to_vec(config)
        .map_err(|_| "无效的上传配置。")?
        .len()
        > 32 * 1024
    {
        return Err("上传配置不能超过 32 KB。".into());
    }
    Ok(())
}
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn replace_tokens(value: &str, bindings: &[(&str, String)]) -> String {
    let mut output = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find("{{") {
        let Some(end_offset) = remaining[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + end_offset;
        output.push_str(&remaining[..start]);
        let key = remaining[start + 2..end].trim();
        if let Some((_, replacement)) = bindings.iter().find(|(name, _)| *name == key) {
            output.push_str(replacement);
        } else {
            output.push_str(&remaining[start..end + 2]);
        }
        remaining = &remaining[end + 2..];
    }
    output.push_str(remaining);
    output
}
fn render_url(template: &str, filename: &str, id: &str) -> Result<String, String> {
    let url = replace_tokens(
        template.trim(),
        &[("filename", encoded(filename)), ("id", encoded(id))],
    );
    if url.contains("{{") {
        return Err("URL 模板仅支持 {{filename}} 和 {{id}}。".into());
    }
    Ok(url)
}
fn render_body(value: &Value, bindings: &[(&str, String)]) -> Value {
    match value {
        Value::String(value) => Value::String(replace_tokens(value, bindings)),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| render_body(value, bindings))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), render_body(value, bindings)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn response_url(
    config: &ImageUploadConfig,
    bytes: &[u8],
    filename: &str,
    id: &str,
) -> Result<String, String> {
    let value = match config.response_type.as_str() {
        "url-template" => render_url(&config.public_url_template, filename, id)?,
        "text" => std::str::from_utf8(bytes)
            .map_err(|_| "上传响应不是有效文本。")?
            .trim()
            .to_owned(),
        "json" => {
            let json: Value = serde_json::from_slice(bytes)
                .map_err(|_| "上传成功，但响应不是 JSON，请检查 URL 提取配置。")?;
            let path = config.response_url_path.trim();
            let found = if path.starts_with('/') {
                json.pointer(path)
            } else {
                path.trim_start_matches("$.")
                    .split('.')
                    .try_fold(&json, |current, part| {
                        if let Some(array) = current.as_array() {
                            part.parse::<usize>()
                                .ok()
                                .and_then(|index| array.get(index))
                        } else {
                            current.get(part)
                        }
                    })
            };
            found
                .and_then(Value::as_str)
                .ok_or("上传响应中找不到 URL 字符串，请检查响应字段路径。")?
                .trim()
                .to_owned()
        }
        _ => return Err("未知的 URL 提取方式。".into()),
    };
    attachments::validate_url(&value)
        .map_err(|_| "返回值不是有效的 HTTP(S) 图片 URL，请检查响应提取配置。".to_string())?;
    Ok(value)
}
pub async fn upload(
    client: &Client,
    config: &ImageUploadConfig,
    bytes: Vec<u8>,
    filename: &str,
    id: &str,
) -> Result<String, String> {
    validate(config)?;
    if bytes.len() > attachments::MAX_IMAGE_BYTES {
        return Err("上传图片不能超过 5 MB。".into());
    }
    let mime = attachments::media_type(&bytes)?;
    let base64 = STANDARD.encode(&bytes);
    let bindings = [
        ("filename", filename.to_owned()),
        ("id", id.to_owned()),
        ("mimeType", mime.to_owned()),
        ("size", bytes.len().to_string()),
        ("base64", base64.clone()),
        ("dataUrl", format!("data:{mime};base64,{base64}")),
    ];
    let body = render_body(&config.body, &bindings);
    let url = render_url(&config.url, filename, id)?;
    let mut request = client.request(
        Method::from_bytes(config.method.as_bytes()).map_err(|_| "无效的上传方法。")?,
        attachments::validate_url(&url)?,
    );
    for (name, value) in &config.headers {
        request = request.header(name, value);
    }
    match config.body_type.as_str() {
        "multipart" => {
            let part = multipart::Part::bytes(bytes)
                .file_name(filename.to_owned())
                .mime_str(mime)
                .map_err(|_| "无效的图片格式。")?;
            let mut form = multipart::Form::new();
            for (key, value) in body.as_object().ok_or("Body 必须是对象。")? {
                if key == &config.file_field {
                    return Err("Body 参数中不能重复配置文件字段。".into());
                }
                form = form.text(
                    key.clone(),
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                );
            }
            request = request.multipart(form.part(config.file_field.clone(), part));
        }
        "json" => {
            request = request.json(&body);
        }
        "form-urlencoded" => {
            let fields: Vec<(String, String)> = body
                .as_object()
                .ok_or("Body 必须是对象。")?
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string()),
                    )
                })
                .collect();
            request = request.form(&fields);
        }
        "binary" => {
            if !config
                .headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("content-type"))
            {
                request = request.header("Content-Type", mime);
            }
            request = request.body(bytes);
        }
        _ => return Err("不支持的上传格式。".into()),
    }
    let response = request
        .send()
        .await
        .map_err(|_| "无法连接图片上传服务，请检查地址、网络或签名有效期。".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "图片上传失败（HTTP {}），请检查上传地址、鉴权 Header 和 Body 参数。",
            response.status().as_u16()
        ));
    }
    let mut result = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "读取图片上传响应失败。")?;
        if result.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("图片上传响应超过大小限制。".into());
        }
        result.extend_from_slice(&chunk);
    }
    response_url(config, &result, filename, id)
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    fn config(url: String, body_type: &str) -> ImageUploadConfig {
        ImageUploadConfig {
            url,
            method: "POST".into(),
            body_type: body_type.into(),
            file_field: "image".into(),
            headers: [("X-Test".into(), "test-secret".into())].into(),
            body: serde_json::json!({"imageData":"{{base64}}","name":"{{filename}}"}),
            response_type: "json".into(),
            response_url_path: "data.links.0.url".into(),
            public_url_template: "".into(),
        }
    }
    #[test]
    fn templates_accept_whitespace_without_interpreting_replacement_values() {
        assert_eq!(
            render_url("https://cdn.test/{{ id }}/{{ filename }}", "a #.png", "123").unwrap(),
            "https://cdn.test/123/a%20%23.png"
        );
        let value = render_body(
            &serde_json::json!({"name":"{{ filename }}","data":"{{base64}}"}),
            &[
                ("filename", "{{base64}}.png".into()),
                ("base64", "image-data".into()),
            ],
        );
        assert_eq!(value["name"], "{{base64}}.png");
        assert_eq!(value["data"], "image-data");
    }
    #[test]
    fn extracts_json_plaintext_and_oss_url_templates() {
        let mut config = config("https://upload.test/upload".into(), "json");
        assert_eq!(
            response_url(
                &config,
                br#"{"data":{"links":[{"url":"https://cdn.test/test.png"}]}}"#,
                "test.png",
                "123"
            )
            .unwrap(),
            "https://cdn.test/test.png"
        );
        config.response_type = "text".into();
        assert!(response_url(&config, b"file:///secret", "test.png", "123").is_err());
        config.response_type = "url-template".into();
        config.public_url_template = "https://cdn.test/{{id}}/{{filename}}".into();
        assert_eq!(
            response_url(&config, b"", "a #.png", "123").unwrap(),
            "https://cdn.test/123/a%20%23.png"
        );
    }
    #[tokio::test]
    async fn uploads_all_body_formats_with_custom_headers_and_parameters() {
        for kind in ["multipart", "json", "form-urlencoded", "binary"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/upload", listener.local_addr().unwrap());
            let mut config = config(url, kind);
            if kind == "binary" {
                config.method = "PUT".into();
            }
            let expected = kind.to_owned();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length:")
                                    .map(|value| value.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() < end + 4 + length {
                            continue;
                        }
                        assert!(headers.contains("x-test: test-secret"));
                        assert!(headers.starts_with(if expected == "binary" {
                            "put /upload "
                        } else {
                            "post /upload "
                        }));
                        let body = &bytes[end + 4..];
                        match expected.as_str() {
                            "multipart" => {
                                let text = String::from_utf8_lossy(body);
                                assert!(headers.contains("multipart/form-data; boundary="));
                                assert!(text.contains("name=\"image\"; filename=\"test.png\""));
                                assert!(text.contains("imageData"));
                            }
                            "json" => {
                                let json: Value = serde_json::from_slice(body).unwrap();
                                assert_eq!(json["imageData"], attachments::TEST_PNG);
                                assert_eq!(json["name"], "test.png");
                            }
                            "form-urlencoded" => {
                                assert!(String::from_utf8_lossy(body).contains("imageData=iVBOR"));
                            }
                            "binary" => {
                                assert!(body.starts_with(b"\x89PNG"));
                            }
                            _ => unreachable!(),
                        }
                        break;
                    }
                }
                let body = r#"{"data":{"links":[{"url":"https://cdn.test/image.png"}]}}"#;
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
            let url = upload(
                &crate::llm::client().unwrap(),
                &config,
                STANDARD.decode(attachments::TEST_PNG).unwrap(),
                "test.png",
                "123",
            )
            .await
            .unwrap();
            assert_eq!(url, "https://cdn.test/image.png");
            server.await.unwrap();
        }
    }
}
