use super::*;
use crate::agent::{tools, types::Risk};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

fn default_client() -> Client {
    Client::builder().no_proxy().build().unwrap()
}
async fn headers(stream: &mut TcpStream) -> String {
    let mut result = Vec::new();
    while !result.ends_with(b"\r\n\r\n") {
        assert!(result.len() < 65536);
        result.push(stream.read_u8().await.unwrap());
    }
    String::from_utf8(result).unwrap()
}
async fn json_response(stream: &mut TcpStream, value: serde_json::Value) {
    let body = value.to_string();
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
}
fn http_tool(url: &str) -> tools::HttpTool {
    tools::HttpTool {
        definition: crate::agent::types::ToolDefinition {
            name: "lookup".into(),
            description: "Test lookup".into(),
            input_schema: json!({"type":"object"}),
            risk: Risk::ReadOnly,
        },
        enabled: true,
        url: url.into(),
        method: "GET".into(),
        headers: Default::default(),
        query: Default::default(),
        body: serde_json::Value::Null,
        response_path: String::new(),
    }
}

#[test]
fn old_configurations_default_to_no_custom_proxy_and_invalid_values_hide_credentials() {
    let legacy: tools::Configuration =
        serde_json::from_value(json!({"clock":true,"http":[],"mcp":[]})).unwrap();
    assert!(!legacy.proxy.enabled);
    legacy.validate().unwrap();
    for url in [
        "http://127.0.0.1:7890",
        "https://localhost:8443",
        "socks5://127.0.0.1:1080",
        "socks5h://[::1]:1080",
        "http://user:private-pass@127.0.0.1:8080",
    ] {
        Configuration {
            enabled: true,
            url: url.into(),
        }
        .validate()
        .unwrap();
    }
    for url in [
        "",
        "127.0.0.1:7890",
        "file:///private",
        "ftp://localhost:1234",
        "http://127.0.0.1:0",
        "socks5h://localhost",
        "http://localhost:7890/proxy",
        "http://user:private-pass@localhost:7890/?token=secret",
        "http://localhost:7890/#fragment",
    ] {
        let settings = Configuration {
            enabled: true,
            url: url.into(),
        };
        let error = settings.validate().unwrap_err();
        assert!(!error.contains("private-pass") && !error.contains("token=secret"));
    }
    Configuration {
        enabled: false,
        url: "invalid draft address".into(),
    }
    .validate()
    .unwrap();
}

#[tokio::test]
async fn http_registry_routes_to_proxy_and_disabling_restores_default_without_mutating_model_client()
 {
    let default_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tool_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let default = Client::builder()
        .no_proxy()
        .proxy(Proxy::all(format!("http://{}", default_listener.local_addr().unwrap())).unwrap())
        .build()
        .unwrap();
    let proxy = Configuration {
        enabled: true,
        url: format!("http://{}", tool_listener.local_addr().unwrap()),
    };
    let default_server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = default_listener.accept().await.unwrap();
            let request = headers(&mut stream).await;
            assert!(
                request.starts_with("GET http://blocked.invalid/"),
                "{request}"
            );
            json_response(&mut stream, json!({"route":"default"})).await;
        }
    });
    let tool_server = tokio::spawn(async move {
        let (mut stream, _) = tool_listener.accept().await.unwrap();
        let request = headers(&mut stream).await;
        assert!(
            request.starts_with("GET http://blocked.invalid/lookup"),
            "{request}"
        );
        json_response(&mut stream, json!({"route":"tool-proxy"})).await;
    });
    let mut config = tools::Configuration {
        proxy,
        http: vec![http_tool("http://blocked.invalid/lookup")],
        ..Default::default()
    };
    let registry = config.registry(&default).unwrap();
    let value = registry.tools["lookup"]
        .execute(&json!({}), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(value["route"], "tool-proxy");
    config.proxy.enabled = false;
    let registry = config.registry(&default).unwrap();
    let value = registry.tools["lookup"]
        .execute(&json!({}), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(value["route"], "default");
    let value: serde_json::Value = default
        .get("http://blocked.invalid/model")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(value["route"], "default");
    tool_server.await.unwrap();
    default_server.await.unwrap();
}

#[tokio::test]
async fn brave_https_uses_http_connect_without_external_traffic_or_auth_leak_in_connect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let settings = Configuration {
        enabled: true,
        url: format!("http://{}", listener.local_addr().unwrap()),
    };
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = headers(&mut stream).await;
        assert!(
            request.starts_with("CONNECT api.search.brave.com:443 HTTP/1.1"),
            "{request}"
        );
        assert!(!request.contains("private-brave-test-key"));
        stream
            .write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
    });
    let mut config = tools::Configuration {
        proxy: settings,
        ..Default::default()
    };
    config.brave.api_key = "private-brave-test-key".into();
    let registry = config.registry(&default_client()).unwrap();
    let error = registry.tools["brave_search"]
        .execute(&json!({"query":"Rust"}), &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(!error.contains("private-brave-test-key"));
    server.await.unwrap();
}

#[tokio::test]
async fn socks5h_sends_remote_hostname_and_returns_tool_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let settings = Configuration {
        enabled: true,
        url: format!("socks5h://{}", listener.local_addr().unwrap()),
    };
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), 5);
        let count = stream.read_u8().await.unwrap() as usize;
        let mut methods = vec![0; count];
        stream.read_exact(&mut methods).await.unwrap();
        assert!(methods.contains(&0));
        stream.write_all(&[5, 0]).await.unwrap();
        let mut request = [0; 4];
        stream.read_exact(&mut request).await.unwrap();
        assert_eq!(request, [5, 1, 0, 3]);
        let count = stream.read_u8().await.unwrap() as usize;
        let mut host = vec![0; count];
        stream.read_exact(&mut host).await.unwrap();
        assert_eq!(host, b"blocked.invalid");
        assert_eq!(stream.read_u16().await.unwrap(), 80);
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
            .await
            .unwrap();
        let header = headers(&mut stream).await;
        let target = header.split_whitespace().nth(1).unwrap();
        assert_eq!(target.split('?').next().unwrap(), "/lookup");
        json_response(&mut stream, json!({"route":"socks5h"})).await;
    });
    let config = tools::Configuration {
        proxy: settings,
        http: vec![http_tool("http://blocked.invalid/lookup")],
        ..Default::default()
    };
    let registry = config.registry(&default_client()).unwrap();
    let value = registry.tools["lookup"]
        .execute(&json!({}), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(value["route"], "socks5h");
    server.await.unwrap();
}

#[tokio::test]
async fn mcp_discovery_and_runtime_calls_share_proxy_routing() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = Configuration {
        enabled: true,
        url: format!("http://{}", listener.local_addr().unwrap()),
    };
    let mock = tokio::spawn(async move {
        let mut methods = Vec::new();
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let header = headers(&mut stream).await;
            assert!(header.starts_with("POST http://mcp.blocked.invalid/mcp HTTP/1.1"));
            let length = header
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            let message: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let method = message["method"].as_str().unwrap();
            methods.push(method.to_string());
            let result = match method {
                "initialize" => {
                    json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"test","version":"1"}})
                }
                "notifications/initialized" => json!({}),
                "tools/list" => {
                    json!({"tools":[{"name":"lookup","description":"Lookup","inputSchema":{"type":"object"}}]})
                }
                "tools/call" => json!({"content":[{"type":"text","text":"MCP via proxy"}]}),
                _ => panic!("Unexpected method {method}"),
            };
            json_response(
                &mut stream,
                json!({"jsonrpc":"2.0","id":message["id"],"result":result}),
            )
            .await;
        }
        methods
    });
    let mut server = tools::McpServer {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Proxy test".into(),
        url: "http://mcp.blocked.invalid/mcp".into(),
        enabled: true,
        headers: Default::default(),
        tools: vec![],
    };
    server.tools =
        crate::agent::transport::discover(proxy.client(&default_client()).unwrap(), server.clone())
            .await
            .unwrap();
    assert_eq!(server.tools.len(), 1);
    server.tools[0].enabled = true;
    server.tools[0].risk = Risk::ReadOnly;
    let name = tools::mcp_name(&server.id, "lookup");
    let config = tools::Configuration {
        proxy,
        mcp: vec![server],
        ..Default::default()
    };
    let registry = config.registry(&default_client()).unwrap();
    let value = registry.tools[&name]
        .execute(&json!({}), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(value["content"][0]["text"], "MCP via proxy");
    assert_eq!(
        mock.await.unwrap(),
        [
            "initialize",
            "notifications/initialized",
            "tools/list",
            "initialize",
            "notifications/initialized",
            "tools/call"
        ]
    );
}

#[tokio::test]
async fn unavailable_proxy_does_not_fall_back_to_a_reachable_direct_target() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = Configuration {
        enabled: true,
        url: format!("http://{}", closed.local_addr().unwrap()),
    };
    drop(closed);
    let client = proxy.client(&default_client()).unwrap();
    assert!(
        client
            .get(format!("http://{}/lookup", target.local_addr().unwrap()))
            .send()
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), target.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn socks5_accepts_ip_targets_without_remote_dns() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = Configuration {
        enabled: true,
        url: format!("socks5://{}", listener.local_addr().unwrap()),
    };
    let mock = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), 5);
        let count = stream.read_u8().await.unwrap() as usize;
        let mut methods = vec![0; count];
        stream.read_exact(&mut methods).await.unwrap();
        stream.write_all(&[5, 0]).await.unwrap();
        let mut request = [0; 10];
        stream.read_exact(&mut request).await.unwrap();
        assert_eq!(request, [5, 1, 0, 1, 127, 0, 0, 2, 0, 80]);
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
            .await
            .unwrap();
        assert!(
            headers(&mut stream)
                .await
                .starts_with("GET /lookup HTTP/1.1")
        );
        json_response(&mut stream, json!({"socks5":true})).await;
    });
    let value: serde_json::Value = proxy
        .client(&default_client())
        .unwrap()
        .get("http://127.0.0.2/lookup")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(value["socks5"], true);
    mock.await.unwrap();
}
