mod agent;
mod attachments;
mod catalog;
mod configuration;
mod credentials;
mod image_upload;
mod llm;
mod models;
mod sse;
mod storage;
mod sync_store;
mod titles;
mod vault;

use base64::{Engine, engine::general_purpose::STANDARD};
use models::{
    AvailableModel, ChatRequest, Connection, ImageAttachment, ImageUploadConfig, StreamEvent,
    Workspace,
};
use std::{collections::HashMap, sync::Mutex};
use tauri::{Manager, State, ipc::Channel};
use tauri_plugin_dialog::DialogExt;
use tokio_util::sync::CancellationToken;

struct Requests {
    client: reqwest::Client,
    running: Mutex<HashMap<String, CancellationToken>>,
    approvals: Mutex<HashMap<String, (String, tokio::sync::oneshot::Sender<bool>)>>,
}
fn local_directory(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|_| "无法定位本机数据目录。".into())
}
fn init_storage(session: &mut sync_store::Session, app: &tauri::AppHandle) -> Result<(), String> {
    let default_directory = app
        .path()
        .home_dir()
        .map_err(|_| "无法定位用户目录。")?
        .join(".oh-my-chatwise");
    session.init_with_default(&local_directory(app)?, &default_directory)?;
    configuration::migrate_local(session, &app.config().identifier)
}
fn data_directory(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let store = app.state::<sync_store::Store>();
    let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
    init_storage(&mut session, app)?;
    session.directory()
}
fn stored_secret(app: &tauri::AppHandle, key: &str) -> Result<Option<String>, String> {
    let store = app.state::<sync_store::Store>();
    let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
    init_storage(&mut session, app)?;
    // Passive reads must not acknowledge remote versions on behalf of an open editor.
    let mut view = session.clone();
    view.refresh_secrets()?;
    configuration::read_secret(&view, &app.config().identifier, key)
}
async fn with_storage<T: Send + 'static>(
    app: tauri::AppHandle,
    action: impl FnOnce(&mut sync_store::Session, &tauri::AppHandle) -> Result<T, String>
    + Send
    + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<sync_store::Store>();
        let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
        init_storage(&mut session, &app)?;
        action(&mut session, &app)
    })
    .await
    .map_err(|_| "存储任务意外结束。".to_string())?
}
#[tauri::command]
async fn load_workspace(app: tauri::AppHandle) -> Result<Option<Workspace>, String> {
    with_storage(app, |session, _app| {
        let previous = session.clone();
        let result = session.load();
        if result.is_err() {
            *session = previous;
        }
        result
    })
    .await
}
#[tauri::command]
async fn save_workspace(app: tauri::AppHandle, workspace: Workspace) -> Result<(), String> {
    with_storage(app, move |session, _| session.save(&workspace)).await
}
#[tauri::command]
async fn save_api_key(
    app: tauri::AppHandle,
    connection_id: String,
    api_key: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<sync_store::Store>();
        let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
        init_storage(&mut session, &app)?;
        if api_key.trim().is_empty() || api_key.len() > 8192 {
            return Err("密钥不能为空或超过 8192 字节。".into());
        }
        configuration::publish(
            &mut session,
            &app.config().identifier,
            &format!("secret/{connection_id}"),
            Some(&api_key),
        )
    })
    .await
    .map_err(|_| "凭据任务意外结束。".to_string())?
}
#[tauri::command]
async fn delete_api_key(app: tauri::AppHandle, connection_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<sync_store::Store>();
        let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
        init_storage(&mut session, &app)?;
        configuration::publish(
            &mut session,
            &app.config().identifier,
            &format!("secret/{connection_id}"),
            None,
        )
    })
    .await
    .map_err(|_| "凭据任务意外结束。".to_string())?
}
#[tauri::command]
async fn storage_status(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    with_storage(app, |session, app| {
        let mut status = serde_json::to_value(session.status()?).unwrap();
        status["credentialsEnabled"] = configuration::password(&app.config().identifier)?.is_some().into();
        status["credentialStorage"] = serde_json::json!({"mode":"file", "path":session.directory()?.join("sync-v1/events").display().to_string()});
        Ok(status)
    })
    .await
}
#[tauri::command]
async fn switch_storage(
    app: tauri::AppHandle,
    directory: String,
    migrate: bool,
    workspace: Workspace,
) -> Result<(), String> {
    with_storage(app, move |session, app| {
        sync_store::records(&workspace)?;
        sync_store::atomic(
            &local_directory(app)?
                .join("recovery")
                .join(format!("{}.json", uuid::Uuid::new_v4())),
            &serde_json::to_vec(&workspace).map_err(|_| "无法创建切换前恢复快照。")?,
            true,
        )?;
        session.switch(&directory, migrate)
    })
    .await
}
#[tauri::command]
async fn configure_sync_password(
    app: tauri::AppHandle,
    password: String,
    workspace: Workspace,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<sync_store::Store>();
        let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
        init_storage(&mut session, &app)?;
        session.refresh_secrets()?;
        let workspace = session.load()?.unwrap_or(workspace);
        configuration::configure_password(
            &mut session,
            &app.config().identifier,
            &password,
            &workspace,
        )
    })
    .await
    .map_err(|_| "配置同步凭据失败。".to_string())?
}
#[tauri::command]
async fn resolve_storage_conflict(
    app: tauri::AppHandle,
    key: String,
    revision: String,
    keep_both: bool,
) -> Result<(), String> {
    with_storage(app, move |session, app| {
        let credential = if keep_both && key.starts_with("connection/") {
            configuration::read_secret(
                session,
                &app.config().identifier,
                &key.replace("connection/", "secret/"),
            )?
        } else {
            None
        };
        let copies = session.resolve(&key, &revision, keep_both)?;
        if let Some(credential) = credential {
            for id in copies {
                configuration::publish(
                    session,
                    &app.config().identifier,
                    &format!("secret/{id}"),
                    Some(&credential),
                )?;
            }
        }
        Ok(())
    })
    .await
}
#[tauri::command]
async fn pick_storage_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("选择 oh-my-chatwise 数据目录")
            .blocking_pick_folder()
            .map(|p| {
                p.into_path()
                    .map(|p| p.display().to_string())
                    .map_err(|_| "请选择本机目录。".to_string())
            })
            .transpose()
    })
    .await
    .map_err(|_| "选择目录失败。".to_string())?
}
#[tauri::command]
async fn save_configuration_file(
    app: tauri::AppHandle,
    workspace: Workspace,
    include_secrets: bool,
    password: String,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let contents = {
            let store = app.state::<sync_store::Store>();
            let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
            init_storage(&mut session, &app)?;
            session.refresh_secrets()?;
            configuration::export(
                &workspace,
                &session,
                &app.config().identifier,
                include_secrets,
                &password,
            )?
        };
        let Some(path) = app
            .dialog()
            .file()
            .set_title("导出 oh-my-chatwise 配置（不含会话）")
            .set_file_name("chatwise-config.json")
            .add_filter("oh-my-chatwise 配置", &["json"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|_| "请选择本机文件路径。")?;
        sync_store::atomic(&path, contents.as_bytes(), false)?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|_| "配置导出任务失败。".to_string())?
}
#[tauri::command]
async fn preview_configuration(
    contents: String,
    password: String,
) -> Result<configuration::Preview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        configuration::decode(&contents, &password).map(|c| configuration::preview(&c))
    })
    .await
    .map_err(|_| "配置读取任务失败。".to_string())?
}
#[tauri::command]
async fn import_configuration(
    app: tauri::AppHandle,
    contents: String,
    password: String,
    replace: bool,
    mut workspace: Workspace,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = configuration::decode(&contents, &password)?;
        let store = app.state::<sync_store::Store>();
        let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
        init_storage(&mut session, &app)?;
        configuration::apply(
            config,
            &mut workspace,
            &app.config().identifier,
            &mut session,
            replace,
        )?;
        session.save(&workspace)
    })
    .await
    .map_err(|_| "配置导入任务失败。".to_string())?
}
#[tauri::command]
async fn fetch_models(
    app: tauri::AppHandle,
    connection: Connection,
    api_key: Option<String>,
    requests: State<'_, Requests>,
) -> Result<Vec<AvailableModel>, String> {
    // Validate the destination before accessing credentials.
    catalog::endpoint(&connection)?;
    let key = match api_key.filter(|key| !key.trim().is_empty()) {
        Some(key) => {
            if key.len() > 8192 {
                return Err("API 密钥超过大小限制。".into());
            }
            Some(key.trim().to_owned())
        }
        None if connection.has_api_key => {
            let id = connection.id.clone();
            let saved = tauri::async_runtime::spawn_blocking(move || {
                stored_secret(&app, &format!("secret/{id}"))
            })
            .await
            .map_err(|_| "读取凭据失败。".to_string())??;
            if saved.is_none() {
                return Err("找不到已保存的密钥，请重新输入 API 密钥。".into());
            }
            saved
        }
        None => None,
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        catalog::fetch(&requests.client, &connection, key.as_deref()),
    )
    .await
    .map_err(|_| "获取模型列表超时，请检查网络后重试。".to_string())?;
    result.map_err(|error| match key.filter(|key| !key.is_empty()) {
        Some(key) => error.replace(&key, "[已隐藏密钥]"),
        None => error,
    })
}
#[tauri::command]
async fn import_image(
    app: tauri::AppHandle,
    name: String,
    data_url: String,
) -> Result<ImageAttachment, String> {
    let directory = data_directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || attachments::import(&directory, &name, &data_url))
        .await
        .map_err(|_| "图片导入任务失败。".to_string())?
}
#[tauri::command]
async fn read_image(app: tauri::AppHandle, id: String) -> Result<String, String> {
    let directory = data_directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || attachments::preview(&directory, &id))
        .await
        .map_err(|_| "图片读取任务失败。".to_string())?
}
#[tauri::command]
async fn load_image_upload_config(
    app: tauri::AppHandle,
) -> Result<Option<ImageUploadConfig>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        stored_secret(&app, "secret/upload")?
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| "已保存的图床配置无法读取。".to_string())
            })
            .transpose()
    })
    .await
    .map_err(|_| "读取图床配置失败。".to_string())?
}
#[tauri::command]
async fn save_image_upload_config(
    app: tauri::AppHandle,
    config: ImageUploadConfig,
) -> Result<(), String> {
    image_upload::validate(&config)?;
    tauri::async_runtime::spawn_blocking(move || {
        let json = serde_json::to_string(&config).map_err(|_| "图床配置无效。")?;
        let store = app.state::<sync_store::Store>();
        let mut session = store.0.lock().map_err(|_| "存储锁不可用。")?;
        configuration::publish(
            &mut session,
            &app.config().identifier,
            "secret/upload",
            Some(&json),
        )
    })
    .await
    .map_err(|_| "保存图床配置失败。".to_string())?
}

#[tauri::command]
async fn upload_image(
    app: tauri::AppHandle,
    id: String,
    name: String,
    requests: State<'_, Requests>,
) -> Result<String, String> {
    let config = load_image_upload_config(app.clone())
        .await?
        .ok_or("请先在设置中配置图床并保存。")?;
    let directory = data_directory(&app)?;
    let read_id = id.clone();
    let bytes =
        tauri::async_runtime::spawn_blocking(move || attachments::read(&directory, &read_id))
            .await
            .map_err(|_| "读取图片失败。".to_string())??;
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        image_upload::upload(&requests.client, &config, bytes, &name, &id),
    )
    .await
    .map_err(|_| "图片上传超时，可重试或改用 Base64 模式。".to_string())?
}
#[tauri::command]
async fn test_image_upload(
    config: ImageUploadConfig,
    requests: State<'_, Requests>,
) -> Result<String, String> {
    let bytes = STANDARD
        .decode(attachments::TEST_PNG)
        .map_err(|_| "无法创建测试图片。".to_string())?;
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        image_upload::upload(
            &requests.client,
            &config,
            bytes,
            "chatwise-test.png",
            &uuid::Uuid::new_v4().to_string(),
        ),
    )
    .await
    .map_err(|_| "图床测试超时，请检查配置和网络。".to_string())?
}
#[tauri::command]
async fn load_tool_configuration(
    app: tauri::AppHandle,
) -> Result<agent::tools::Configuration, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let value = stored_secret(&app, "secret/tools")?;
        value
            .map(|s| serde_json::from_str(&s).map_err(|_| "工具配置格式有误。".to_string()))
            .transpose()
            .map(|c| c.unwrap_or_default())
    })
    .await
    .map_err(|_| "工具配置读取失败。".to_string())?
}
#[tauri::command]
async fn tool_availability(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let config = load_tool_configuration(app).await?;
    config.validate()?;
    let configured = config.configured_names();
    let mut tools = configured.clone();
    tools.push("current_time".into());
    tools.push("calculate".into());
    let web: Vec<_> = configured
        .iter()
        .filter(|n| n.as_str() == "brave_search")
        .cloned()
        .collect();
    let http: Vec<_> = config
        .http
        .iter()
        .filter(|t| t.enabled)
        .map(|t| t.definition.name.clone())
        .collect();
    let mcp: Vec<_> = config
        .mcp
        .iter()
        .filter(|s| s.enabled)
        .flat_map(|s| {
            s.tools
                .iter()
                .filter(|t| t.enabled)
                .map(|t| agent::tools::mcp_name(&s.id, &t.name))
        })
        .collect();
    Ok(
        serde_json::json!({"configured":!configured.is_empty(),"tools":tools,"categories":{"web":web,"http":http,"mcp":mcp}}),
    )
}
#[tauri::command]
async fn save_tool_configuration(
    app: tauri::AppHandle,
    config: agent::tools::Configuration,
) -> Result<(), String> {
    config.validate()?;
    with_storage(app, move |session, app| {
        let value = serde_json::to_string(&config).map_err(|_| "工具配置无效。")?;
        configuration::publish(
            session,
            &app.config().identifier,
            "secret/tools",
            Some(&value),
        )
    })
    .await
}
#[tauri::command]
async fn discover_mcp_tools(
    server: agent::tools::McpServer,
    requests: State<'_, Requests>,
) -> Result<Vec<agent::tools::McpTool>, String> {
    agent::transport::validate_endpoint(&server.url, &server.headers)?;
    agent::transport::discover(requests.client.clone(), server).await
}
#[tauri::command]
async fn test_brave_search(
    api_key: String,
    requests: State<'_, Requests>,
) -> Result<serde_json::Value, String> {
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent::brave::search(
            &requests.client,
            &api_key,
            &serde_json::json!({"query":"Brave Search","count":3}),
        ),
    )
    .await
    .map_err(|_| "Brave Search 测试超时，请检查网络。".to_string())?
}
#[tauri::command]
async fn test_http_tool(
    config: agent::tools::HttpTool,
    args: serde_json::Value,
    requests: State<'_, Requests>,
) -> Result<serde_json::Value, String> {
    if config.definition.risk != agent::types::Risk::ReadOnly {
        return Err("此工具需要在对话中逐次授权后执行。".into());
    }
    agent::tools::Configuration {
        clock: false,
        calculator: false,
        brave: Default::default(),
        http: vec![config.clone()],
        mcp: vec![],
    }
    .validate()?;
    agent::tools::validate_arguments(&args, &config.definition.input_schema)?;
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent::transport::http_tool(&requests.client, &config, &args),
    )
    .await
    .map_err(|_| "工具测试超时。".to_string())?
}
#[tauri::command]
fn approve_tool(
    id: String,
    approval_id: String,
    allow: bool,
    requests: State<'_, Requests>,
) -> Result<(), String> {
    let mut approvals = requests
        .approvals
        .lock()
        .map_err(|_| "工具授权锁不可用。")?;
    if approvals
        .get(&approval_id)
        .is_none_or(|(run, _)| run != &id)
    {
        return Err("此工具授权已失效。".into());
    }
    let (_, sender) = approvals.remove(&approval_id).unwrap();
    sender
        .send(allow)
        .map_err(|_| "运行已结束，此授权不会执行工具。".into())
}
struct DesktopToolPolicy {
    app: tauri::AppHandle,
    run_id: String,
    channel: Channel<StreamEvent>,
}
impl agent::types::ToolPolicy for DesktopToolPolicy {
    fn approve<'a>(
        &'a self,
        call: &'a agent::types::ToolCall,
        definition: &'a agent::types::ToolDefinition,
        target: Option<&'a str>,
        cancel: &'a CancellationToken,
    ) -> agent::types::Task<'a, bool> {
        Box::pin(async move {
            if agent::types::ReadOnlyPolicy
                .approve(call, definition, target, cancel)
                .await?
            {
                return Ok(true);
            }
            let approval_id = uuid::Uuid::new_v4().to_string();
            let (sender, receiver) = tokio::sync::oneshot::channel();
            self.app
                .state::<Requests>()
                .approvals
                .lock()
                .map_err(|_| "授权锁不可用。")?
                .insert(approval_id.clone(), (self.run_id.clone(), sender));
            if self
                .channel
                .send(StreamEvent::ToolApproval {
                    approval_id: approval_id.clone(),
                    call: call.clone(),
                    description: definition.description.clone(),
                    target: target.unwrap_or("本机").into(),
                })
                .is_err()
            {
                self.app
                    .state::<Requests>()
                    .approvals
                    .lock()
                    .map_err(|_| "授权锁不可用。")?
                    .remove(&approval_id);
                return Err("聊天窗口已关闭。".into());
            }
            let answer = tokio::select! {biased;_=cancel.cancelled()=>false,result=tokio::time::timeout(std::time::Duration::from_secs(120),receiver)=>result.ok().and_then(Result::ok).unwrap_or(false)};
            self.app
                .state::<Requests>()
                .approvals
                .lock()
                .map_err(|_| "授权锁不可用。")?
                .remove(&approval_id);
            Ok(answer)
        })
    }
}
#[tauri::command]
fn cancel_chat(id: String, requests: State<'_, Requests>) -> Result<(), String> {
    if let Some(token) = requests
        .running
        .lock()
        .map_err(|_| "请求锁不可用。".to_string())?
        .get(&id)
    {
        token.cancel();
    }
    Ok(())
}
#[tauri::command]
async fn generate_title(
    app: tauri::AppHandle,
    connection: Connection,
    model: String,
    messages: Vec<titles::TitleMessage>,
    requests: State<'_, Requests>,
) -> Result<String, String> {
    let id = connection.id.clone();
    let credential_app = app.clone();
    let key = tauri::async_runtime::spawn_blocking(move || {
        stored_secret(&credential_app, &format!("secret/{id}"))
    })
    .await
    .map_err(|_| "无法读取标题模型凭据。")??;
    if connection.has_api_key && key.is_none() {
        return Err("模型密钥不可用。".into());
    }
    titles::generate(
        &requests.client,
        connection,
        model,
        messages,
        key.as_deref(),
    )
    .await
    .map_err(|error| {
        if let Some(key) = key.filter(|value| !value.is_empty()) {
            error.replace(&key, "[已隐藏密钥]")
        } else {
            error
        }
    })
}
#[tauri::command]
async fn stream_chat(
    mut request: ChatRequest,
    app: tauri::AppHandle,
    channel: Channel<StreamEvent>,
    requests: State<'_, Requests>,
) -> Result<String, String> {
    let token = CancellationToken::new();
    {
        let mut running = requests
            .running
            .lock()
            .map_err(|_| "请求锁不可用。".to_string())?;
        if running.contains_key(&request.id) {
            return Err("请求正在进行中。".into());
        }
        running.insert(request.id.clone(), token.clone());
    }
    let request_id = request.id.clone();
    let result = tokio::select! {
        biased;
        _ = token.cancelled() => Ok("stopped".to_string()),
        result = async {
            let id = request.connection.id.clone();
            let credential_app=app.clone();
            let key = tauri::async_runtime::spawn_blocking(move || stored_secret(&credential_app,&format!("secret/{id}"))).await.map_err(|_| "读取凭据失败。".to_string())??;
            if request.connection.has_api_key && key.is_none() { return Err("未找到已保存的密钥，请检查数据目录及凭据同步状态，或在连接设置中重新填写。".to_string()); }
            let directory = data_directory(&app)?;
            request = tauri::async_runtime::spawn_blocking(move || { attachments::resolve(&directory, &mut request.messages)?; Ok::<_, String>(request) }).await.map_err(|_| "读取图片附件失败。".to_string())??;
            let config = load_tool_configuration(app.clone()).await?.for_conversation(&request.tools);
            let outcome={
                request.agent.validate()?;
                let registry=config.registry(&requests.client)?;
                let model=agent::provider::HttpModel{client:&requests.client,request:&request,key:key.as_deref()};
                let policy=DesktopToolPolicy{app:app.clone(),run_id:request_id.clone(),channel:channel.clone()};
                agent::run(&model,&registry,&policy,&request.agent,&token,&mut |event|channel.send(event).map_err(|_|"聊天窗口已关闭。".to_string())).await
            };
            outcome.map_err(|error| if let Some(key) = key.filter(|key| !key.is_empty()) { error.replace(&key, "[已隐藏密钥]") } else { error })?;
            Ok("complete".to_string())
        } => result,
    };
    requests
        .running
        .lock()
        .map_err(|_| "请求锁不可用。".to_string())?
        .remove(&request_id);
    requests
        .approvals
        .lock()
        .map_err(|_| "授权锁不可用。")?
        .retain(|_, (run, _)| run != &request_id);
    result
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            credentials::initialize(
                &app.config().identifier,
                &local_directory(app.handle()).map_err(std::io::Error::other)?,
            )
            .map_err(std::io::Error::other)?;
            Ok(())
        })
        .manage(sync_store::Store(
            Mutex::new(sync_store::Session::default()),
        ))
        .manage(Requests {
            client: llm::client().expect("Failed to initialize HTTP client"),
            running: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            load_workspace,
            save_workspace,
            storage_status,
            switch_storage,
            configure_sync_password,
            resolve_storage_conflict,
            pick_storage_directory,
            save_configuration_file,
            preview_configuration,
            import_configuration,
            save_api_key,
            delete_api_key,
            fetch_models,
            import_image,
            read_image,
            load_image_upload_config,
            save_image_upload_config,
            upload_image,
            test_image_upload,
            stream_chat,
            generate_title,
            cancel_chat,
            load_tool_configuration,
            tool_availability,
            save_tool_configuration,
            discover_mcp_tools,
            approve_tool,
            test_http_tool,
            test_brave_search
        ])
        .run(tauri::generate_context!())
        .expect("error while running oh-my-chatwise");
}
