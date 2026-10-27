//! Exercise the real protocol parser, continuation payload and loop together, without network or keys.
use super::{
    provider::{self, Capture},
    *,
};
use crate::models::{ChatRequest, Protocol};
use serde_json::{Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

struct ReplayModel {
    request: ChatRequest,
    bodies: Mutex<Vec<Value>>,
}
impl ModelClient for ReplayModel {
    fn protocol(&self) -> Protocol {
        self.request.connection.protocol.clone()
    }
    fn identity(&self) -> String {
        "connection:model".into()
    }
    fn generate<'a>(
        &'a self,
        steps: &'a [AgentStep],
        tools: &'a [ToolDefinition],
        emit: &'a mut Emit<'_>,
    ) -> Task<'a, ModelResponse> {
        Box::pin(async move {
            self.bodies
                .lock()
                .unwrap()
                .push(provider::payload(&self.request, steps, tools)?);
            let mut capture = Capture::default();
            for data in fixtures(&self.protocol(), !steps.is_empty()) {
                capture.ingest(&self.protocol(), &data, emit)?;
            }
            capture.finish(&self.protocol())
        })
    }
}
struct Search {
    definition: ToolDefinition,
    calls: Arc<AtomicUsize>,
}
impl Tool for Search {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }
    fn execute<'a>(&'a self, args: &'a Value, _: &'a CancellationToken) -> Task<'a, Value> {
        Box::pin(async move {
            assert_eq!(args["query"], "Rust news");
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(
                json!({"provider":"Brave Search","results":[{"title":"Rust News","url":"https://www.rust-lang.org/","description":"Latest release"}]}),
            )
        })
    }
}
fn fixtures(protocol: &Protocol, final_round: bool) -> Vec<String> {
    let args = r#"{"query":"Rust news"}"#;
    let output = "Found https://www.rust-lang.org/";
    let events = match protocol {
        Protocol::ChatCompletions => vec![if final_round {
            json!({"choices":[{"delta":{"content":output},"finish_reason":"stop"}]})
        } else {
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"brave_search","arguments":args}}]},"finish_reason":"tool_calls"}]})
        }],
        Protocol::Responses => {
            let item = json!({"type":"function_call","id":"fc-1","call_id":"call-1","name":"brave_search","arguments":args,"status":"completed"});
            if final_round {
                vec![
                    json!({"type":"response.output_text.delta","delta":output}),
                    json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":output}]}]}}),
                ]
            } else {
                vec![
                    json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc-1","call_id":"call-1","name":"brave_search","arguments":""}}),
                    json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":args}),
                    json!({"type":"response.output_item.done","output_index":0,"item":item}),
                    json!({"type":"response.completed","response":{"status":"completed","output":[item]}}),
                ]
            }
        }
        Protocol::Anthropic => vec![
            json!({"type":"content_block_start","index":0,"content_block": if final_round { json!({"type":"text","text":""}) } else { json!({"type":"tool_use","id":"call-1","name":"brave_search","input":{}}) }}),
            json!({"type":"content_block_delta","index":0,"delta": if final_round { json!({"type":"text_delta","text":output}) } else {json!({"type":"input_json_delta","partial_json":args})}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":if final_round { "end_turn" } else { "tool_use" }}}),
            json!({"type":"message_stop"}),
        ],
    };
    let mut events: Vec<String> = events.into_iter().map(|v| v.to_string()).collect();
    if *protocol == Protocol::ChatCompletions {
        events.push("[DONE]".into());
    }
    events
}
#[tokio::test]
async fn configured_search_enters_automatic_loop_and_replays_results_for_all_three_protocols() {
    for protocol in [
        Protocol::ChatCompletions,
        Protocol::Responses,
        Protocol::Anthropic,
    ] {
        let request: ChatRequest = serde_json::from_value(json!({"id":"run","connection":{"id":"connection","name":"test","protocol":protocol,"baseUrl":"https://model.invalid/v1","models":["model"]},"model":"model","messages":[{"role":"user","content":"Search Rust news"}],"systemPrompt":"","maxTokens":1024,"temperature":null,"agent":{"enabled":false}})).unwrap();
        let config = tools::Configuration {
            brave: brave::Configuration {
                api_key: "private-test-key".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            !config
                .for_conversation(&request.tools)
                .configured_names()
                .is_empty()
        );
        let settings = request.agent.clone();
        let model = ReplayModel {
            request,
            bodies: Mutex::new(vec![]),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::default();
        registry
            .register(Arc::new(Search {
                definition: brave::definition(),
                calls: calls.clone(),
            }))
            .unwrap();
        let mut events = vec![];
        run(
            &model,
            &registry,
            &ReadOnlyPolicy,
            &settings,
            &CancellationToken::new(),
            &mut |e| {
                events.push(e);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let bodies = model.bodies.lock().unwrap();
        assert_eq!(bodies.len(), 2);
        for body in bodies.iter() {
            match protocol {
                Protocol::ChatCompletions => assert_eq!(body["reasoning_effort"], "medium"),
                Protocol::Responses => assert_eq!(body["reasoning"]["effort"], "medium"),
                Protocol::Anthropic => assert_eq!(body["output_config"]["effort"], "medium"),
            }
        }
        assert!(bodies[0]["tools"].to_string().contains("brave_search"));
        assert!(
            !serde_json::to_string(&*bodies)
                .unwrap()
                .contains("private-test-key")
        );
        let key = if protocol == Protocol::Responses {
            "input"
        } else {
            "messages"
        };
        assert!(
            bodies[1][key]
                .to_string()
                .contains("https://www.rust-lang.org/")
        );
        assert!(bodies[1][key].to_string().contains("call-1"));
        match protocol {
            Protocol::Anthropic => assert_eq!(bodies[0]["tool_choice"], json!({"type":"auto"})),
            _ => assert_eq!(bodies[0]["tool_choice"], "auto"),
        }
        assert!(events.iter().any(|e| matches!(e, StreamEvent::AgentStep {step} if step.number==2 && step.status=="complete" && step.content.contains("https://www.rust-lang.org/"))));
    }
}
#[test]
fn conversation_switches_filter_external_tools_and_keep_builtins() {
    let config: tools::Configuration = serde_json::from_value(json!({
        "clock":false,"calculator":false,"brave":{"enabled":true,"apiKey":"private-key"},
        "http":[{"definition":{"name":"lookup","description":"lookup","inputSchema":{"type":"object"},"risk":"read-only"},"enabled":true,"url":"https://example.invalid/query","method":"GET","headers":{},"query":{},"body":{},"responsePath":""}],
        "mcp":[{"id":"19be092e-7d74-464a-b7c5-fc4c2f6d94b5","name":"test","url":"https://mcp.invalid","enabled":true,"headers":{},"tools":[{"name":"list","description":"list","inputSchema":{"type":"object"},"enabled":true,"risk":"read-only"}]}]
    })).unwrap();
    let client = reqwest::Client::new();
    for web in [false, true] {
        for http in [false, true] {
            for mcp in [false, true] {
                let filtered =
                    config.for_conversation(&crate::models::ConversationTools { web, http, mcp });
                let registry = filtered.registry(&client).unwrap();
                let names: Vec<_> = registry.tools.keys().map(String::as_str).collect();
                assert!(names.contains(&"current_time") && names.contains(&"calculate"));
                assert_eq!(names.contains(&"brave_search"), web);
                assert_eq!(names.contains(&"lookup"), http);
                assert_eq!(names.iter().any(|n| n.starts_with("mcp_")), mcp);
            }
        }
    }
    assert!(!config.clock && config.http.len() == 1); // Other chats keep their original configuration.
}
