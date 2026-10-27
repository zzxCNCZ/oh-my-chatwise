//! Rust port of the bounded loop and explicit registry in tmc-agent-kernel.
pub mod brave;
#[cfg(test)]
mod loop_tests;
pub mod provider;
pub mod tools;
pub mod transport;
pub mod types;
use crate::models::StreamEvent;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use types::*;

#[derive(Default)]
pub struct Registry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}
impl Registry {
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<(), String> {
        let definition = tool.definition();
        if !valid_name(&definition.name) || definition.input_schema["type"] != "object" {
            return Err("工具名或顶层对象 schema 无效。".into());
        }
        if self.tools.contains_key(&definition.name) {
            return Err(format!("工具名重复：{}", definition.name));
        }
        self.tools.insert(definition.name.clone(), tool);
        Ok(())
    }
}
pub async fn run(
    model: &dyn ModelClient,
    registry: &Registry,
    policy: &dyn ToolPolicy,
    settings: &AgentSettings,
    cancellation: &CancellationToken,
    emit: &mut Emit<'_>,
) -> Result<(), String> {
    settings.validate()?;
    let work = run_loop(model, registry, policy, settings, cancellation, emit);
    tokio::select! { biased;
        _ = cancellation.cancelled() => Err("Agent 已停止。".into()),
        result=tokio::time::timeout(Duration::from_secs(settings.run_timeout_seconds),work) => result.map_err(|_|"Agent 已达到总运行时间上限。".to_string())?,
    }
}
async fn run_loop(
    model: &dyn ModelClient,
    registry: &Registry,
    policy: &dyn ToolPolicy,
    settings: &AgentSettings,
    cancellation: &CancellationToken,
    emit: &mut Emit<'_>,
) -> Result<(), String> {
    let mut steps = Vec::new();
    let mut used = 0;
    let mut per_tool = BTreeMap::<String, u32>::new();
    let mut call_ids = BTreeSet::new();
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut trace_bytes = 0usize;
    for number in 1..=settings.max_steps {
        if cancellation.is_cancelled() {
            return Err("Agent 已停止。".into());
        }
        let available = registry
            .tools
            .values()
            .filter(|t| {
                used < settings.max_tool_calls
                    && per_tool.get(&t.definition().name).copied().unwrap_or(0)
                        < settings
                            .tool_call_limits
                            .get(&t.definition().name)
                            .copied()
                            .unwrap_or(64)
            })
            .map(|t| t.definition().clone())
            .collect::<Vec<_>>();
        let mut step = AgentStep {
            number,
            protocol: model.protocol(),
            model_key: model.identity(),
            content: String::new(),
            reasoning: String::new(),
            calls: vec![],
            results: vec![],
            status: "model".into(),
            provider_items: vec![],
        };
        emit(StreamEvent::AgentStep { step: step.clone() })?;
        let response = model.generate(&steps, &available, emit).await?;
        // Validate the entire batch before executing anything, including duplicate IDs across rounds.
        let mut batch_ids = BTreeSet::new();
        if response.calls.len() > 64
            || response.calls.iter().any(|c| {
                c.id.trim().is_empty()
                    || c.id.len() > 256
                    || !valid_name(&c.name)
                    || !c.arguments.is_object()
                    || !batch_ids.insert(c.id.clone())
                    || call_ids.contains(&c.id)
            })
        {
            return Err("模型返回了无效或重复的工具调用，未执行该批次。".into());
        }
        call_ids.extend(batch_ids);
        input_tokens = input_tokens.saturating_add(response.input_tokens);
        output_tokens = output_tokens.saturating_add(response.output_tokens);
        emit(StreamEvent::Usage {
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
        })?;
        step.content = response.content;
        step.reasoning = response.reasoning;
        step.calls = response.calls;
        step.provider_items = response.provider_items;
        // Leave enough room for every bounded result before starting any side effects.
        let checkpoint_bytes = serde_json::to_vec(&step)
            .map_err(|_| "工具记录无法序列化。")?
            .len();
        let reserved_bytes = checkpoint_bytes.saturating_add(step.calls.len() * (64 * 1024 + 1024));
        if trace_bytes.saturating_add(reserved_bytes) > 12 * 1024 * 1024 {
            return Err(
                "本次工具记录达到 12 MB 上限，未执行当前批次；请新建会话或减少工具返回内容。"
                    .into(),
            );
        }
        if step.calls.is_empty() {
            step.status = "complete".into();
            emit(StreamEvent::AgentStep { step: step.clone() })?;
            if step.content.trim().is_empty() {
                return Err("Agent 最终回复为空。".into());
            }
            return Ok(());
        }
        step.status = "tools".into();
        emit(StreamEvent::AgentStep { step: step.clone() })?;
        let mut batch_count = BTreeMap::<String, u32>::new();
        for call in &step.calls {
            *batch_count.entry(call.name.clone()).or_default() += 1;
        }
        let batch_remaining = settings.max_tool_calls - used;
        let previous_per_tool = per_tool.clone();
        for call in &step.calls {
            if cancellation.is_cancelled() {
                return Err("Agent 已停止。".into());
            }
            let count = batch_count[&call.name];
            let rejected = if step.calls.len() as u32 > batch_remaining {
                Some("本批次超过剩余工具调用总量，整批未执行。")
            } else if count
                > settings
                    .tool_call_step_limits
                    .get(&call.name)
                    .copied()
                    .unwrap_or(64)
            {
                Some("该工具的本批次调用数超过单轮限额，同名调用均未执行。")
            } else if count
                > settings
                    .tool_call_limits
                    .get(&call.name)
                    .copied()
                    .unwrap_or(64)
                    - previous_per_tool.get(&call.name).copied().unwrap_or(0)
            {
                Some("该工具的本批次调用数超过剩余限额，同名调用均未执行。")
            } else {
                None
            };
            let result = if let Some(reason) = rejected {
                ToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    content: reason.into(),
                    status: "rejected".into(),
                }
            } else {
                used += 1;
                *per_tool.entry(call.name.clone()).or_default() += 1;
                execute(
                    registry,
                    policy,
                    call,
                    settings.tool_timeout_seconds,
                    cancellation,
                    emit,
                )
                .await?
            };
            step.results.push(result);
            emit(StreamEvent::AgentStep { step: step.clone() })?;
        }
        step.status = "complete".into();
        emit(StreamEvent::AgentStep { step: step.clone() })?;
        trace_bytes = trace_bytes.saturating_add(
            serde_json::to_vec(&step)
                .map_err(|_| "工具记录无法序列化。")?
                .len(),
        );
        steps.push(step);
    }
    Err(format!(
        "Agent 已达到 {} 次模型请求上限。工具记录已保留。",
        settings.max_steps
    ))
}
async fn execute(
    registry: &Registry,
    policy: &dyn ToolPolicy,
    call: &ToolCall,
    timeout: u64,
    cancellation: &CancellationToken,
    emit: &mut Emit<'_>,
) -> Result<ToolResult, String> {
    let mut result = ToolResult {
        id: call.id.clone(),
        name: call.name.clone(),
        content: String::new(),
        status: "failed".into(),
    };
    let Some(tool) = registry.tools.get(&call.name) else {
        result.content = "工具未注册或未启用。".into();
        return Ok(result);
    };
    if let Err(error) = tools::validate_arguments(&call.arguments, &tool.definition().input_schema)
    {
        result.content = error;
        return Ok(result);
    }
    let target = tool.target();
    let approved = tokio::select! {biased; _=cancellation.cancelled()=>return Err("Agent 已停止。".into()),answer=policy.approve(call,tool.definition(),target.as_deref(),cancellation)=>answer?};
    if !approved {
        result.status = "rejected".into();
        result.content = "用户或执行策略拒绝了此工具调用。".into();
        return Ok(result);
    }
    if cancellation.is_cancelled() {
        return Err("Agent 已停止。".into());
    }
    emit(StreamEvent::ToolStarted {
        call_id: call.id.clone(),
    })?;
    let execution = tokio::select! {biased; _=cancellation.cancelled()=>return Err("Agent 已停止。".into()),value=tokio::time::timeout(Duration::from_secs(timeout),tool.execute(&call.arguments,cancellation))=>value};
    match execution {
        Ok(Ok(value)) => {
            let output = serde_json::to_string(&value).map_err(|_| "工具结果无法序列化。")?;
            result.content = if output.len() > 64 * 1024 {
                format!(
                    "{}\n[工具输出已截断]",
                    output.chars().take(16000).collect::<String>()
                )
            } else {
                output
            };
            result.status = "success".into();
        }
        Ok(Err(error)) => result.content = error.chars().take(2000).collect(),
        Err(_) => result.content = "工具执行超时，未自动重试。".into(),
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Protocol;
    use serde_json::{Value, json};
    use std::{
        collections::VecDeque,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    struct FakeModel {
        rounds: Mutex<VecDeque<ModelResponse>>,
        observed: Mutex<Vec<(usize, usize)>>,
    }
    impl ModelClient for FakeModel {
        fn protocol(&self) -> Protocol {
            Protocol::ChatCompletions
        }
        fn generate<'a>(
            &'a self,
            steps: &'a [AgentStep],
            tools: &'a [ToolDefinition],
            emit: &'a mut Emit<'_>,
        ) -> Task<'a, ModelResponse> {
            Box::pin(async move {
                assert!(steps.iter().all(|s| s.calls.len() == s.results.len()));
                self.observed
                    .lock()
                    .unwrap()
                    .push((steps.len(), tools.len()));
                let response = self
                    .rounds
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or("unexpected round")?;
                if !response.content.is_empty() {
                    emit(StreamEvent::Text {
                        text: response.content.clone(),
                    })?;
                }
                Ok(response)
            })
        }
    }
    struct CounterTool {
        definition: ToolDefinition,
        count: Arc<AtomicUsize>,
        slow: bool,
    }
    impl Tool for CounterTool {
        fn definition(&self) -> &ToolDefinition {
            &self.definition
        }
        fn execute<'a>(
            &'a self,
            args: &'a Value,
            _cancel: &'a CancellationToken,
        ) -> Task<'a, Value> {
            Box::pin(async move {
                self.count.fetch_add(1, Ordering::SeqCst);
                if self.slow {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
                Ok(args.clone())
            })
        }
    }
    fn setup(
        rounds: Vec<ModelResponse>,
        risk: Risk,
        slow: bool,
    ) -> (FakeModel, Registry, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let mut registry = Registry::default();
        registry.register(Arc::new(CounterTool{definition:ToolDefinition{name:"lookup".into(),description:"lookup".into(),input_schema:json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}),risk},count:count.clone(),slow})).unwrap();
        (
            FakeModel {
                rounds: Mutex::new(rounds.into()),
                observed: Mutex::new(vec![]),
            },
            registry,
            count,
        )
    }
    fn calls(ids: &[&str]) -> ModelResponse {
        ModelResponse {
            calls: ids
                .iter()
                .map(|id| ToolCall {
                    id: (*id).into(),
                    name: "lookup".into(),
                    arguments: json!({"query":"hello"}),
                })
                .collect(),
            input_tokens: 2,
            output_tokens: 3,
            ..Default::default()
        }
    }
    fn answer() -> ModelResponse {
        ModelResponse {
            content: "Final answer".into(),
            input_tokens: 5,
            output_tokens: 7,
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn executes_whole_batch_then_returns_results_and_aggregated_usage() {
        let (model, registry, count) = setup(
            vec![calls(&["one", "two"]), answer()],
            Risk::ReadOnly,
            false,
        );
        let mut events = vec![];
        let settings = AgentSettings {
            max_tool_calls: 2,
            tool_call_limits: BTreeMap::from([("lookup".into(), 2)]),
            ..Default::default()
        };
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
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(*model.observed.lock().unwrap(), vec![(0, 1), (1, 0)]);
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::Usage {
                input_tokens: Some(7),
                output_tokens: Some(10)
            }
        )));
    }
    #[tokio::test]
    async fn rejects_oversized_batch_without_partial_execution() {
        let (model, registry, count) = setup(
            vec![calls(&["one", "two"]), answer()],
            Risk::ReadOnly,
            false,
        );
        let mut rejected = 0;
        run(
            &model,
            &registry,
            &ReadOnlyPolicy,
            &AgentSettings {
                max_tool_calls: 1,
                ..Default::default()
            },
            &CancellationToken::new(),
            &mut |e| {
                if let StreamEvent::AgentStep { step } = e {
                    rejected = rejected.max(
                        step.results
                            .iter()
                            .filter(|r| r.status == "rejected")
                            .count(),
                    );
                }
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(rejected, 2);
    }
    #[tokio::test]
    async fn per_step_limit_rejects_whole_same_tool_group() {
        let (model, registry, count) = setup(
            vec![calls(&["one", "two"]), answer()],
            Risk::ReadOnly,
            false,
        );
        run(
            &model,
            &registry,
            &ReadOnlyPolicy,
            &AgentSettings {
                tool_call_step_limits: BTreeMap::from([("lookup".into(), 1)]),
                ..Default::default()
            },
            &CancellationToken::new(),
            &mut |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn duplicate_ids_never_execute_a_partial_batch() {
        let (model, registry, count) = setup(
            vec![calls(&["duplicate", "duplicate"])],
            Risk::ReadOnly,
            false,
        );
        assert!(
            run(
                &model,
                &registry,
                &ReadOnlyPolicy,
                &AgentSettings::default(),
                &CancellationToken::new(),
                &mut |_| Ok(())
            )
            .await
            .unwrap_err()
            .contains("重复")
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn denied_external_actions_return_evidence_without_execution() {
        let (model, registry, count) =
            setup(vec![calls(&["one"]), answer()], Risk::ExternalAction, false);
        run(
            &model,
            &registry,
            &ReadOnlyPolicy,
            &AgentSettings::default(),
            &CancellationToken::new(),
            &mut |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn schema_errors_become_model_evidence_and_allow_recovery() {
        let mut bad = calls(&["bad"]);
        bad.calls[0].arguments = json!({"query":42});
        let (model, registry, count) =
            setup(vec![bad, calls(&["good"]), answer()], Risk::ReadOnly, false);
        run(
            &model,
            &registry,
            &ReadOnlyPolicy,
            &AgentSettings::default(),
            &CancellationToken::new(),
            &mut |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn limits_and_empty_output_are_explicit_failures() {
        let (model, registry, _) = setup(vec![calls(&["one"])], Risk::ReadOnly, false);
        assert!(
            run(
                &model,
                &registry,
                &ReadOnlyPolicy,
                &AgentSettings {
                    max_steps: 1,
                    ..Default::default()
                },
                &CancellationToken::new(),
                &mut |_| Ok(())
            )
            .await
            .unwrap_err()
            .contains("上限")
        );
        let (model, registry, _) = setup(vec![ModelResponse::default()], Risk::ReadOnly, false);
        assert!(
            run(
                &model,
                &registry,
                &ReadOnlyPolicy,
                &AgentSettings::default(),
                &CancellationToken::new(),
                &mut |_| Ok(())
            )
            .await
            .unwrap_err()
            .contains("为空")
        );
    }
    #[tokio::test]
    async fn cancellation_drops_current_tool_and_never_starts_next() {
        let (model, registry, count) = setup(vec![calls(&["one", "two"])], Risk::ReadOnly, true);
        let token = CancellationToken::new();
        let cancel = token.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel.cancel();
        });
        assert!(
            run(
                &model,
                &registry,
                &ReadOnlyPolicy,
                &AgentSettings::default(),
                &token,
                &mut |_| Ok(())
            )
            .await
            .is_err()
        );
        task.await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn tool_timeout_is_evidence_and_does_not_retry() {
        let (model, registry, count) = setup(vec![calls(&["one"]), answer()], Risk::ReadOnly, true);
        let mut timed_out = false;
        run(
            &model,
            &registry,
            &ReadOnlyPolicy,
            &AgentSettings {
                tool_timeout_seconds: 1,
                ..Default::default()
            },
            &CancellationToken::new(),
            &mut |e| {
                if let StreamEvent::AgentStep { step } = e {
                    timed_out |= step
                        .results
                        .iter()
                        .any(|r| r.status == "failed" && r.content.contains("超时"));
                }
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(timed_out);
    }
}
