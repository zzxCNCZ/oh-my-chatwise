use crate::{
    credentials, image_upload,
    models::{Connection, ImageSettings, ImageUploadConfig, Workspace},
    sync_store::Session,
    vault,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PASSWORD_KEY: &str = "storage-sync-password";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub connections: Vec<Connection>,
    pub default_connection_id: String,
    pub default_model: String,
    pub theme: String,
    pub image_settings: ImageSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<crate::models::GenerationSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<crate::models::Prompt>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_prompt_id: Option<String>,
    #[serde(default)]
    pub api_keys: BTreeMap<String, String>,
    #[serde(default)]
    pub image_upload: Option<ImageUploadConfig>,
    #[serde(default)]
    pub tool_configuration: Option<crate::agent::tools::Configuration>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Package {
    format: String,
    version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    encrypted: Option<vault::Sealed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<Config>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub connection_names: Vec<String>,
    pub key_count: usize,
    pub has_image_upload: bool,
    pub tool_count: usize,
    pub prompt_count: usize,
    pub has_generation: bool,
    pub theme: String,
}
pub fn password(service: &str) -> Result<Option<String>, String> {
    credentials::read(service, PASSWORD_KEY)
}
pub fn read_secret(session: &Session, service: &str, key: &str) -> Result<Option<String>, String> {
    session
        .secret(key)
        .map(|value| {
            if let Some(text) = value.as_str() {
                return Ok(text.to_owned());
            }
            let password = password(service)?
                .ok_or("此目录包含加密凭据，请在存储与同步中输入原同步密码解锁。")?;
            let sealed: vault::Sealed =
                serde_json::from_value(value).map_err(|_| "同步凭据格式有误。")?;
            String::from_utf8(vault::open(&sealed, &password)?)
                .map_err(|_| "同步凭据内容有误。".into())
        })
        .transpose()
}
fn encode_secret(value: &str, password: Option<&str>) -> Result<serde_json::Value, String> {
    if let Some(password) = password {
        serde_json::to_value(vault::seal(value.as_bytes(), password)?)
            .map_err(|_| "无法编码加密凭据。".into())
    } else {
        Ok(serde_json::Value::String(value.into()))
    }
}
pub fn publish(
    session: &mut Session,
    service: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), String> {
    if value.is_some_and(|v| v.is_empty() || v.len() > 256 * 1024) {
        return Err("凭据为空或超过 256 KB。".into());
    }
    if session.has_record(key) && read_secret(session, service, key)?.as_deref() == value {
        return Ok(());
    }
    let password = password(service)?;
    let encoded = value
        .map(|v| encode_secret(v, password.as_deref()))
        .transpose()?;
    session.write_secret(key, encoded)
}
/// Upgrade already-migrated local files without ever loading OS credentials.
/// Existing records (including deletions) always win over the recovery snapshot.
pub fn migrate_local(session: &mut Session, service: &str) -> Result<(), String> {
    let Some(entries) = credentials::pending_migration(service)? else {
        return Ok(());
    };
    // Convert workspace.json before adding secrets, so an old workspace is not hidden.
    session.load()?;
    let password = password(service)?;
    let mut values = Vec::new();
    for (id, value) in entries {
        let key = match id.as_str() {
            "tool-configuration" => "secret/tools".into(),
            image_upload::CONFIG_KEY => "secret/upload".into(),
            _ => format!("secret/{id}"),
        };
        if !session.has_record(&key) {
            values.push((key, Some(encode_secret(&value, password.as_deref())?)));
        }
    }
    session.write_secrets(values)?;
    credentials::finish_migration(service)
}
pub fn configure_password(
    session: &mut Session,
    service: &str,
    pass: &str,
    _workspace: &Workspace,
) -> Result<(), String> {
    if !pass.is_empty() && (pass.len() < 8 || pass.len() > 1024) {
        return Err("同步密码需要 8–1024 字节。".into());
    }
    let previous = password(service)?;
    if !pass.is_empty() && previous.as_deref().is_some_and(|old| old != pass) {
        return Err("此目录已使用其他同步密码，请使用原密码或切换独立目录。".into());
    }
    // Decode every current record before changing any state. A denied/missing password
    // or corrupt record cannot silently discard credentials when disabling encryption.
    let mut values = Vec::new();
    for key in session.secret_keys() {
        let Some(raw) = session.secret(&key) else {
            continue;
        };
        let raw_is_plain = raw.is_string();
        let value = if let Some(text) = raw.as_str() {
            text.to_owned()
        } else {
            let sealed = serde_json::from_value(raw).map_err(|_| "同步凭据格式有误。")?;
            let unlock = if pass.is_empty() {
                previous
                    .as_deref()
                    .ok_or("请输入原同步密码解锁后再关闭加密。")?
            } else {
                pass
            };
            String::from_utf8(vault::open(&sealed, unlock)?).map_err(|_| "同步凭据内容有误。")?
        };
        let target_password = if pass.is_empty() { None } else { Some(pass) };
        // Unlocking an already encrypted record does not rewrite or resolve its heads.
        if raw_is_plain != pass.is_empty() {
            let encoded = encode_secret(&value, target_password)?;
            values.push((key, Some(encoded)));
        }
    }
    if !values.is_empty()
        && session
            .status()?
            .conflicts
            .iter()
            .any(|conflict| conflict.key.starts_with("secret/"))
    {
        return Err("请先解决凭据同步冲突，再切换凭据加密方式。".into());
    }
    if pass.is_empty() {
        credentials::delete(service, PASSWORD_KEY)?;
    } else {
        credentials::save(service, PASSWORD_KEY, pass)?;
    }
    if let Err(error) = session.write_secrets(values) {
        let rollback = if let Some(old) = previous {
            credentials::save(service, PASSWORD_KEY, &old)
        } else {
            credentials::delete(service, PASSWORD_KEY)
        };
        return Err(if rollback.is_err() {
            format!("{error} 本机同步密码恢复失败，请重新解锁。")
        } else {
            error
        });
    }
    Ok(())
}
fn validate(config: &Config) -> Result<(), String> {
    if config.connections.len() > 1000
        || !matches!(config.theme.as_str(), "dark" | "light" | "system")
        || !matches!(config.image_settings.mode.as_str(), "base64" | "url")
    {
        return Err("配置内容或数量不受支持。".into());
    }
    if let Some(generation) = &config.generation {
        generation.validate()?;
    }
    if let Some(prompts) = &config.prompts {
        let mut ids = std::collections::BTreeSet::new();
        if prompts.len() > 1000
            || prompts.iter().any(|p| {
                p.id.is_empty()
                    || p.id.len() > 120
                    || !p.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                    || !ids.insert(&p.id)
                    || p.name.trim().is_empty()
                    || p.name.chars().count() > 200
                    || p.content.len() > 256 * 1024
            })
        {
            return Err("提示词配置无效。".into());
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    for c in &config.connections {
        if c.id.is_empty()
            || c.id.len() > 100
            || !c.id.chars().all(|v| v.is_ascii_alphanumeric() || v == '-')
            || !ids.insert(&c.id)
            || c.name.trim().is_empty()
        {
            return Err("模型连接 ID 重复或配置无效。".into());
        }
        crate::catalog::endpoint(c)?;
    }
    for (id, key) in &config.api_keys {
        if !ids.contains(id) || key.trim().is_empty() || key.len() > 8192 {
            return Err("配置中的密钥无效。".into());
        }
    }
    if !config.default_connection_id.is_empty() {
        let c = config
            .connections
            .iter()
            .find(|c| c.id == config.default_connection_id)
            .ok_or("默认连接不在导入列表中。")?;
        if !c.models.contains(&config.default_model) {
            return Err("默认模型不在连接中。".into());
        }
    }
    if let Some(upload) = &config.image_upload {
        image_upload::validate(upload)?;
    }
    if let Some(tools) = &config.tool_configuration {
        tools.validate()?;
    }
    Ok(())
}
pub fn export(
    workspace: &Workspace,
    session: &Session,
    service: &str,
    include_secrets: bool,
    pass: &str,
) -> Result<String, String> {
    let mut config = Config {
        connections: workspace.connections.clone(),
        default_connection_id: workspace.default_connection_id.clone(),
        default_model: workspace.default_model.clone(),
        theme: workspace.theme.clone(),
        image_settings: workspace.image_settings.clone(),
        generation: workspace.generation.clone(),
        prompts: workspace.prompts.clone(),
        default_prompt_id: workspace.default_prompt_id.clone(),
        api_keys: BTreeMap::new(),
        image_upload: None,
        tool_configuration: None,
    };
    for c in &mut config.connections {
        c.has_api_key = false;
        if include_secrets
            && let Some(key) = read_secret(session, service, &format!("secret/{}", c.id))?
        {
            config.api_keys.insert(c.id.clone(), key);
            c.has_api_key = true;
        }
    }
    config.image_settings.has_upload_config = false;
    if include_secrets {
        config.image_upload = read_secret(session, service, "secret/upload")?
            .map(|s| serde_json::from_str(&s).map_err(|_| "图床配置损坏。".to_string()))
            .transpose()?;
        config.image_settings.has_upload_config = config.image_upload.is_some();
        config.tool_configuration = read_secret(session, service, "secret/tools")?
            .map(|s| serde_json::from_str(&s).map_err(|_| "工具配置损坏。".to_string()))
            .transpose()?;
    }
    validate(&config)?;
    let (encrypted, config) = if include_secrets {
        (
            Some(vault::seal(&serde_json::to_vec(&config).unwrap(), pass)?),
            None,
        )
    } else {
        (None, Some(config))
    };
    let encoded = serde_json::to_string_pretty(&Package {
        format: "chatwise-config".into(),
        version: 1,
        encrypted,
        config,
    })
    .map_err(|_| "无法生成配置文件。".to_string())?;
    if encoded.len() > 4 * 1024 * 1024 {
        return Err("导出配置超过 4 MB，请减少模型配置后再导出。".into());
    }
    Ok(encoded)
}
pub fn decode(contents: &str, pass: &str) -> Result<Config, String> {
    if contents.len() > 4 * 1024 * 1024 {
        return Err("配置文件超过 4 MB。".into());
    }
    let package: Package = serde_json::from_str(contents)
        .map_err(|_| "不是有效的配置文件（配置文件不能包含会话）。")?;
    if package.format != "chatwise-config" || package.version != 1 {
        return Err("不支持此配置文件版本。".into());
    }
    let config = match (package.encrypted, package.config) {
        (Some(sealed), None) => serde_json::from_slice(&vault::open(&sealed, pass)?)
            .map_err(|_| "加密配置格式有误。")?,
        (None, Some(config)) => {
            if !config.api_keys.is_empty()
                || config.image_upload.is_some()
                || config.tool_configuration.is_some()
            {
                return Err("含凭据的配置必须使用加密格式。".into());
            }
            config
        }
        _ => return Err("配置文件格式有误。".into()),
    };
    validate(&config)?;
    Ok(config)
}
pub fn preview(config: &Config) -> Preview {
    Preview {
        connection_names: config.connections.iter().map(|c| c.name.clone()).collect(),
        key_count: config.api_keys.len(),
        has_image_upload: config.image_upload.is_some(),
        tool_count: config
            .tool_configuration
            .as_ref()
            .map(|c| {
                usize::from(c.clock)
                    + usize::from(c.calculator)
                    + usize::from(c.brave.active())
                    + c.http.len()
                    + c.mcp.iter().map(|s| s.tools.len()).sum::<usize>()
            })
            .unwrap_or(0),
        theme: config.theme.clone(),
        prompt_count: config.prompts.as_ref().map_or(0, Vec::len),
        has_generation: config.generation.is_some(),
    }
}
pub fn apply(
    config: Config,
    workspace: &mut Workspace,
    service: &str,
    session: &mut Session,
    replace: bool,
) -> Result<(), String> {
    validate(&config)?;
    let password = if config.api_keys.is_empty()
        && config.image_upload.is_none()
        && config.tool_configuration.is_none()
    {
        None
    } else {
        password(service)?
    };
    let mut values = Vec::new();
    for (id, key) in &config.api_keys {
        values.push((
            format!("secret/{id}"),
            Some(encode_secret(key, password.as_deref())?),
        ));
    }
    if let Some(upload) = &config.image_upload {
        values.push((
            "secret/upload".into(),
            Some(encode_secret(
                &serde_json::to_string(upload).unwrap(),
                password.as_deref(),
            )?),
        ));
    }
    if let Some(tools) = &config.tool_configuration {
        values.push((
            "secret/tools".into(),
            Some(encode_secret(
                &serde_json::to_string(tools).unwrap(),
                password.as_deref(),
            )?),
        ));
    }
    session.write_secrets(values)?;
    if replace {
        workspace.connections = config.connections;
    } else {
        for c in config.connections {
            if let Some(old) = workspace.connections.iter_mut().find(|old| old.id == c.id) {
                *old = c;
            } else {
                workspace.connections.push(c);
            }
        }
    }
    workspace.default_connection_id = config.default_connection_id;
    workspace.default_model = config.default_model;
    workspace.theme = config.theme;
    workspace.image_settings = config.image_settings;
    if let Some(generation) = config.generation {
        workspace.generation = Some(generation);
    }
    if let Some(prompts) = config.prompts {
        if replace {
            workspace.prompts = Some(prompts);
        } else {
            let existing = workspace.prompts.get_or_insert_with(Vec::new);
            for prompt in prompts {
                if let Some(old) = existing.iter_mut().find(|p| p.id == prompt.id) {
                    *old = prompt;
                } else {
                    existing.push(prompt);
                }
            }
        }
    }
    if let Some(id) = config.default_prompt_id {
        workspace.default_prompt_id = Some(id);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_credentials_keep_encrypted_sync_and_portable_transfer_working() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("local");
        let service = format!("file-source-{}", uuid::Uuid::new_v4());
        credentials::initialize(&service, &local).unwrap();
        let mut session = Session::default();
        session
            .init_with_default(&local, &root.path().join("shared"))
            .unwrap();
        let mut workspace:Workspace=serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[],"defaultConnectionId":"","defaultModel":"","theme":"dark"})).unwrap();
        let mut config = fixture();
        config.tool_configuration = Some(crate::agent::tools::Configuration {
            brave: crate::agent::brave::Configuration {
                api_key: "test-brave-key".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        apply(config, &mut workspace, &service, &mut session, true).unwrap();
        configure_password(&mut session, &service, "sync-password", &workspace).unwrap();
        assert_eq!(
            read_secret(&session, &service, "secret/test-id").unwrap(),
            Some("sample-key".into())
        );
        let exported = export(&workspace, &session, &service, true, "export-password").unwrap();
        assert!(!exported.contains("sample-key"));
        assert!(!exported.contains("test-brave-key"));
        let restored = decode(&exported, "export-password").unwrap();
        let target = format!("file-target-{}", uuid::Uuid::new_v4());
        credentials::initialize(&target, &root.path().join("target")).unwrap();
        let mut target_session = Session::default();
        target_session.init(&root.path().join("target")).unwrap();
        apply(restored, &mut workspace, &target, &mut target_session, true).unwrap();
        assert_eq!(
            read_secret(&target_session, &target, "secret/test-id").unwrap(),
            Some("sample-key".into())
        );
        let tools = read_secret(&target_session, &target, "secret/tools")
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&tools).unwrap()["brave"]["apiKey"],
            "test-brave-key"
        );
        assert!(credentials::read(&target, PASSWORD_KEY).unwrap().is_none());
    }
    fn device(root: &std::path::Path, name: &str, shared: &std::path::Path) -> (String, Session) {
        let local = root.join(name);
        let service = uuid::Uuid::new_v4().to_string();
        credentials::initialize(&service, &local).unwrap();
        let mut session = Session::default();
        session.init_with_default(&local, shared).unwrap();
        migrate_local(&mut session, &service).unwrap();
        session.load().unwrap();
        (service, session)
    }
    fn replicate(source: &std::path::Path, target: &std::path::Path) {
        for entry in std::fs::read_dir(source.join("sync-v1/events")).unwrap() {
            let entry = entry.unwrap();
            let path = target.join("sync-v1/events").join(entry.file_name());
            if !path.exists() {
                crate::sync_store::atomic(&path, &std::fs::read(entry.path()).unwrap(), true)
                    .unwrap();
            }
        }
    }
    #[test]
    fn plaintext_credentials_and_tools_work_on_new_device_without_password_or_local_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let (a, mut first) = device(root.path(), "a", &shared);
        let mut workspace: Workspace = serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[],"defaultConnectionId":"","defaultModel":"","theme":"light"})).unwrap();
        let mut config = fixture();
        config.tool_configuration = Some(crate::agent::tools::Configuration {
            brave: crate::agent::brave::Configuration {
                api_key: "brave-fixture".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        apply(config, &mut workspace, &a, &mut first, true).unwrap();
        first.save(&workspace).unwrap();
        let (b, second) = device(root.path(), "b", &shared);
        assert!(password(&b).unwrap().is_none());
        assert_eq!(
            read_secret(&second, &b, "secret/test-id")
                .unwrap()
                .as_deref(),
            Some("sample-key")
        );
        assert!(
            read_secret(&second, &b, "secret/tools")
                .unwrap()
                .unwrap()
                .contains("brave-fixture")
        );
        assert!(
            read_secret(&second, &b, "secret/upload")
                .unwrap()
                .unwrap()
                .contains("sample-upload-key")
        );
        assert!(!root.path().join("b/credentials.json").exists());
        let portable = export(&workspace, &second, &b, true, "export-password").unwrap();
        assert_eq!(
            decode(&portable, "export-password").unwrap().api_keys["test-id"],
            "sample-key"
        );
    }
    #[test]
    fn offline_credential_edits_keep_both_versions_and_hide_values_in_conflicts() {
        let root = tempfile::tempdir().unwrap();
        let left = root.path().join("left");
        let right = root.path().join("right");
        let (a, mut first) = device(root.path(), "a", &left);
        let (b, mut second) = device(root.path(), "b", &right);
        publish(&mut first, &a, "secret/shared-id", Some("left-key")).unwrap();
        publish(&mut second, &b, "secret/shared-id", Some("right-key")).unwrap();
        publish(&mut first, &a, "secret/only-a", Some("a-key")).unwrap();
        publish(&mut second, &b, "secret/only-b", Some("b-key")).unwrap();
        replicate(&left, &right);
        replicate(&right, &left);
        first.refresh_secrets().unwrap();
        second.refresh_secrets().unwrap();
        let status = first.status().unwrap();
        assert_eq!(status.conflicts.len(), 1);
        let preview = serde_json::to_string(&status).unwrap();
        assert!(!preview.contains("left-key"));
        assert!(!preview.contains("right-key"));
        assert_eq!(
            read_secret(&first, &a, "secret/only-b").unwrap().as_deref(),
            Some("b-key")
        );
        let selected = status.conflicts[0].variants[0].revision.clone();
        first.resolve("secret/shared-id", &selected, false).unwrap();
        replicate(&left, &right);
        first.refresh_secrets().unwrap();
        second.refresh_secrets().unwrap();
        assert_eq!(
            read_secret(&first, &a, "secret/shared-id").unwrap(),
            read_secret(&second, &b, "secret/shared-id").unwrap()
        );
        assert!(second.status().unwrap().conflicts.is_empty());
    }
    #[test]
    fn upgrade_preserves_legacy_workspace_tombstones_and_does_not_resurrect_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("local");
        let shared = root.path().join("shared");
        let service = uuid::Uuid::new_v4().to_string();
        credentials::initialize(&service, &local).unwrap();
        credentials::save(&service, "test-id", "old-key").unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        let mut workspace: Workspace = serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[],"defaultConnectionId":"","defaultModel":"","theme":"dark"})).unwrap();
        workspace.connections = fixture().connections;
        crate::storage::save(&shared, &workspace).unwrap();
        let mut session = Session::default();
        session.init_with_default(&local, &shared).unwrap();
        migrate_local(&mut session, &service).unwrap();
        assert_eq!(session.load().unwrap().unwrap().connections.len(), 1);
        assert_eq!(
            read_secret(&session, &service, "secret/test-id")
                .unwrap()
                .as_deref(),
            Some("old-key")
        );
        publish(&mut session, &service, "secret/test-id", None).unwrap();
        migrate_local(&mut session, &service).unwrap();
        assert!(
            read_secret(&session, &service, "secret/test-id")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            credentials::read(&service, "test-id").unwrap().as_deref(),
            Some("old-key")
        );
        // Even a failed marker write / retry must respect existing deletion records.
        std::fs::remove_file(local.join("credential-records-migrated.json")).unwrap();
        migrate_local(&mut session, &service).unwrap();
        assert!(
            read_secret(&session, &service, "secret/test-id")
                .unwrap()
                .is_none()
        );
        let empty = root.path().join("empty");
        session.switch(empty.to_str().unwrap(), false).unwrap();
        session.load().unwrap();
        migrate_local(&mut session, &service).unwrap();
        assert!(
            read_secret(&session, &service, "secret/test-id")
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn encrypted_legacy_directory_unlocks_and_can_return_to_plaintext_without_key_loss() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let (a, mut first) = device(root.path(), "a", &shared);
        publish(&mut first, &a, "secret/test-id", Some("private-key")).unwrap();
        let workspace: Workspace = serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[],"defaultConnectionId":"","defaultModel":"","theme":"light"})).unwrap();
        configure_password(&mut first, &a, "sync-password", &workspace).unwrap();
        let (b, mut second) = device(root.path(), "b", &shared);
        assert!(read_secret(&second, &b, "secret/test-id").is_err());
        let before = second.secret("secret/test-id");
        assert!(configure_password(&mut second, &b, "wrong-password", &workspace).is_err());
        assert!(password(&b).unwrap().is_none());
        assert_eq!(before, second.secret("secret/test-id"));
        assert!(configure_password(&mut second, &b, "", &workspace).is_err());
        configure_password(&mut second, &b, "sync-password", &workspace).unwrap();
        assert_eq!(
            read_secret(&second, &b, "secret/test-id")
                .unwrap()
                .as_deref(),
            Some("private-key")
        );
        configure_password(&mut second, &b, "", &workspace).unwrap();
        assert!(password(&b).unwrap().is_none());
        assert_eq!(
            second.secret("secret/test-id").unwrap().as_str(),
            Some("private-key")
        );
        first.refresh_secrets().unwrap();
        assert_eq!(
            read_secret(&first, &a, "secret/test-id")
                .unwrap()
                .as_deref(),
            Some("private-key")
        );
    }
    #[test]
    fn encryption_switch_does_not_resolve_conflicting_credentials() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let (a, mut first) = device(root.path(), "a", &shared);
        let (b, mut second) = device(root.path(), "b", &shared);
        publish(&mut first, &a, "secret/test-id", Some("first")).unwrap();
        publish(&mut second, &b, "secret/test-id", Some("second")).unwrap();
        first.refresh_secrets().unwrap();
        let workspace: Workspace = serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[],"defaultConnectionId":"","defaultModel":"","theme":"light"})).unwrap();
        assert!(configure_password(&mut first, &a, "sync-password", &workspace).is_err());
        assert_eq!(first.status().unwrap().conflicts[0].variants.len(), 2);
        assert!(password(&a).unwrap().is_none());
    }
    #[test]
    fn credential_batch_validation_commits_nothing_on_invalid_key() {
        let root = tempfile::tempdir().unwrap();
        let (service, mut session) = device(root.path(), "device", &root.path().join("shared"));
        assert!(
            session
                .write_secrets(vec![
                    ("secret/good".into(), Some(serde_json::json!("sample"))),
                    ("chat/invalid".into(), None)
                ])
                .is_err()
        );
        session.refresh_secrets().unwrap();
        assert!(
            read_secret(&session, &service, "secret/good")
                .unwrap()
                .is_none()
        );
    }
    fn fixture() -> Config {
        serde_json::from_value(serde_json::json!({"connections":[{"id":"test-id","name":"自定义连接","protocol":"responses","baseUrl":"https://api.example.test/v1","models":["custom"],"modelNames":{"custom":"别名"},"hasApiKey":true}],"defaultConnectionId":"test-id","defaultModel":"custom","theme":"light","imageSettings":{"mode":"url","hasUploadConfig":true},"apiKeys":{"test-id":"sample-key"},"imageUpload":{"url":"https://upload.example.test","method":"POST","bodyType":"multipart","fileField":"file","headers":{"Authorization":"sample-upload-key"},"body":{},"responseType":"json","responseUrlPath":"data.url","publicUrlTemplate":""}})).unwrap()
    }
    #[test]
    fn portable_encrypted_config_restores_all_fields_without_conversations() {
        let mut config = fixture();
        let tools: crate::agent::tools::Configuration = serde_json::from_value(serde_json::json!({
            "clock":true,"calculator":true,"brave":{"enabled":true,"apiKey":"private-brave-key"},"http":[{
                "definition":{"name":"web_search","description":"Search","inputSchema":{"type":"object"},"risk":"read-only"},
                "enabled":true,"url":"https://search.example.test/query","method":"POST",
                "headers":{"Authorization":"Bearer private-search-key"},"query":{},"body":{},"responsePath":""
            }],"mcp":[]
        })).unwrap();
        config.tool_configuration = Some(tools);
        validate(&config).unwrap();
        let encrypted =
            vault::seal(&serde_json::to_vec(&config).unwrap(), "portable-password").unwrap();
        let package = serde_json::to_string(&Package {
            format: "chatwise-config".into(),
            version: 1,
            encrypted: Some(encrypted),
            config: None,
        })
        .unwrap();
        assert!(!package.contains("sample-key"));
        assert!(!package.contains("api.example"));
        assert!(!package.contains("conversations"));
        assert!(!package.contains("private-search-key"));
        assert!(!package.contains("private-brave-key"));
        let restored = decode(&package, "portable-password").unwrap();
        assert_eq!(restored.api_keys["test-id"], "sample-key");
        assert_eq!(restored.connections[0].model_names["custom"], "别名");
        let tools = restored.tool_configuration.unwrap();
        assert_eq!(tools.brave.api_key, "private-brave-key");
        assert_eq!(
            tools.http[0].headers["Authorization"],
            "Bearer private-search-key"
        );
        assert_eq!(
            restored.image_upload.unwrap().headers["Authorization"],
            "sample-upload-key"
        );
        assert!(decode(&package, "incorrect-password").is_err());
    }
    #[test]
    fn exports_and_imports_prompts_generation_and_model_effort_without_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let service = format!("settings-test-{}", uuid::Uuid::new_v4());
        credentials::initialize(&service, &dir.path().join("local")).unwrap();
        let mut session = Session::default();
        session.init(&dir.path().join("local")).unwrap();
        let mut workspace: Workspace = serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[],"defaultConnectionId":"","defaultModel":"","theme":"light"})).unwrap();
        let mut config = fixture();
        config.api_keys.clear();
        config.image_upload = None;
        config.prompts = Some(vec![crate::models::Prompt {
            id: "test-prompt".into(),
            name: "中文提示词".repeat(20),
            content: "Write clearly".into(),
        }]);
        config.default_prompt_id = Some("test-prompt".into());
        config.generation = Some(crate::models::GenerationSettings {
            max_tokens: 8192,
            temperature: Some(0.4),
            ..Default::default()
        });
        config.connections[0]
            .model_reasoning
            .insert("custom".into(), crate::models::ReasoningEffort::High);
        apply(config, &mut workspace, &service, &mut session, true).unwrap();
        let encrypted = export(
            &workspace,
            &session,
            &service,
            true,
            "settings-test-password",
        )
        .unwrap();
        assert!(!encrypted.contains("Write clearly"));
        let decrypted = decode(&encrypted, "settings-test-password").unwrap();
        assert_eq!(
            decrypted.prompts.as_ref().unwrap()[0].content,
            "Write clearly"
        );
        assert_eq!(decrypted.generation.as_ref().unwrap().max_tokens, 8192);
        let exported = export(&workspace, &session, &service, false, "").unwrap();
        assert!(!exported.contains("conversations"));
        let restored = decode(&exported, "").unwrap();
        assert_eq!(
            restored.prompts.as_ref().unwrap()[0].content,
            "Write clearly"
        );
        assert_eq!(restored.default_prompt_id.as_deref(), Some("test-prompt"));
        assert_eq!(restored.generation.as_ref().unwrap().max_tokens, 8192);
        assert_eq!(
            restored.connections[0].model_reasoning["custom"],
            crate::models::ReasoningEffort::High
        );
        workspace.prompts = Some(vec![]);
        apply(restored, &mut workspace, &service, &mut session, true).unwrap();
        assert_eq!(workspace.prompts.unwrap()[0].name, "中文提示词".repeat(20));
    }
    #[test]
    fn importing_public_configuration_keeps_sessions_and_drafts_intact() {
        let mut workspace:Workspace=serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[{"id":"chat","title":"must keep","messages":[],"createdAt":"now","updatedAt":"now","pinned":true,"connectionId":"old","model":"old-model","systemPrompt":"keep prompt","maxTokens":4096,"temperature":null,"draft":"keep draft"}],"activeConversationId":"chat","defaultConnectionId":"","defaultModel":"","theme":"dark"})).unwrap();
        let before = serde_json::to_value(&workspace.conversations).unwrap();
        let mut config = fixture();
        config.api_keys.clear();
        config.image_upload = None;
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::default();
        session.init(dir.path()).unwrap();
        apply(
            config,
            &mut workspace,
            "unit-tests-no-credentials",
            &mut session,
            true,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(workspace.conversations).unwrap(),
            before
        );
        assert_eq!(workspace.active_conversation_id.as_deref(), Some("chat"));
        assert_eq!(workspace.theme, "light");
        assert_eq!(workspace.default_model, "custom");
    }
    #[test]
    fn rejects_workspace_files_and_plaintext_credentials() {
        assert!(decode(r#"{"version":1,"conversations":[]}"#, "").is_err());
        let config = Config {
            connections: vec![],
            default_connection_id: "".into(),
            default_model: "".into(),
            theme: "dark".into(),
            image_settings: ImageSettings::default(),
            generation: None,
            prompts: None,
            default_prompt_id: None,
            api_keys: BTreeMap::new(),
            image_upload: None,
            tool_configuration: None,
        };
        let package = Package {
            format: "chatwise-config".into(),
            version: 1,
            encrypted: None,
            config: Some(config),
        };
        let mut value = serde_json::to_value(package).unwrap();
        assert!(decode(&value.to_string(), "").is_ok());
        value["config"]["toolConfiguration"] =
            serde_json::to_value(crate::agent::tools::Configuration::default()).unwrap();
        assert!(decode(&value.to_string(), "").is_err());
        value["config"]["toolConfiguration"] = serde_json::Value::Null;
        value["config"]["conversations"] = serde_json::json!([]);
        assert!(decode(&value.to_string(), "").is_err());
    }
}
