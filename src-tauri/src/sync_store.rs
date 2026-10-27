//! Immutable, causally linked per-record revisions. No shared file is overwritten.
use crate::models::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
type Records = BTreeMap<String, Value>;
type Heads = BTreeMap<String, Vec<Revision>>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub id: String,
    pub device: String,
    pub key: String,
    pub supersedes: Vec<String>,
    pub value: Option<Value>,
}
#[derive(Serialize, Deserialize)]
struct Event {
    version: u8,
    revisions: Vec<Revision>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    version: u8,
    directory: Option<PathBuf>,
    device: String,
    active_conversation_id: Option<String>,
}
#[derive(Clone, Default)]
pub struct Session {
    local: Option<PathBuf>,
    default_directory: Option<PathBuf>,
    location: Option<Location>,
    baseline: Records,
    heads: Heads,
    fingerprint: String,
}
pub struct Store(pub Mutex<Session>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub revision: String,
    pub device: String,
    pub deleted: bool,
    pub preview: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub key: String,
    pub label: String,
    pub variants: Vec<Variant>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub directory: String,
    pub default_directory: String,
    pub device: String,
    pub pending: bool,
    pub conflicts: Vec<Conflict>,
    pub revision_count: usize,
}

pub fn atomic(path: &Path, bytes: &[u8], immutable: bool) -> Result<(), String> {
    let parent = path.parent().ok_or("无效的存储路径。")?;
    fs::create_dir_all(parent).map_err(|e| format!("无法创建存储目录：{e}"))?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("无法创建存储文件：{e}"))?;
    temp.write_all(bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|e| format!("无法写入存储文件：{e}"))?;
    if immutable {
        temp.persist_noclobber(path)
            .map_err(|e| format!("无法创建变更记录：{e}"))?;
    } else {
        temp.persist(path)
            .map_err(|e| format!("无法保存本机存储位置：{e}"))?;
    }
    Ok(())
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("无法读取数据文件：{e}"))?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err("数据文件无效或超过 32 MB，未覆盖原文件。".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("无法读取数据文件：{e}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| format!("数据文件格式有误，已保留原文件：{}", path.display()))
}
fn valid_key(key: &str) -> bool {
    key.strip_prefix("chat/")
        .or_else(|| key.strip_prefix("connection/"))
        .or_else(|| key.strip_prefix("prompt/"))
        .or_else(|| key.strip_prefix("secret/"))
        .is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 120
                && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        || matches!(
            key,
            "preference/theme"
                | "preference/defaultConnectionId"
                | "preference/defaultModel"
                | "preference/imageSettings"
                | "preference/generation"
                | "preference/defaultPromptId"
                | "secret/upload"
        )
}
fn scan(directory: &Path) -> Result<(Heads, String, usize), String> {
    if !directory.is_dir() {
        return Err("数据目录暂不可用。请等待同步盘挂载，或在设置中切换目录。".into());
    }
    let dir = directory.join("sync-v1/events");
    let mut all = BTreeMap::<String, Revision>::new();
    let mut size = 0;
    if dir.exists() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("无法读取同步记录：{e}"))? {
            let entry = entry.map_err(|e| format!("无法读取同步记录：{e}"))?;
            if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            size += entry
                .metadata()
                .map_err(|_| "无法读取同步记录大小。")?
                .len();
            if size > MAX_TOTAL {
                return Err("同步历史超过 512 MB，请备份目录后整理历史；未丢弃任何记录。".into());
            }
            let event: Event = read_json(&entry.path())?;
            if event.version != 1 {
                return Err("同步目录含更高版本的数据，请升级客户端。".into());
            }
            for rev in event.revisions {
                if uuid::Uuid::parse_str(&rev.id).is_err()
                    || uuid::Uuid::parse_str(&rev.device).is_err()
                    || !valid_key(&rev.key)
                    || rev.supersedes.len() > 10000
                    || rev
                        .supersedes
                        .iter()
                        .any(|id| uuid::Uuid::parse_str(id).is_err() || id == &rev.id)
                {
                    return Err("同步变更记录无效，未覆盖。".into());
                }
                if let Some(old) = all.get(&rev.id) {
                    if serde_json::to_value(old).unwrap() != serde_json::to_value(&rev).unwrap() {
                        return Err(
                            "同步软件产生了内容不同的同 ID 文件，请保留副本并检查同步目录。".into(),
                        );
                    }
                } else {
                    all.insert(rev.id.clone(), rev);
                }
            }
        }
    }
    // Reject cross-record causal references, including those arriving out of order later.
    for rev in all.values() {
        for parent in &rev.supersedes {
            if all.get(parent).is_some_and(|old| old.key != rev.key) {
                return Err("同步记录的因果关系无效。".into());
            }
        }
    }
    let removed: BTreeSet<_> = all
        .values()
        .flat_map(|r| r.supersedes.iter().cloned())
        .collect();
    let fingerprint = all.keys().cloned().collect::<Vec<_>>().join(",");
    let count = all.len();
    let mut heads = Heads::new();
    for rev in all.into_values().filter(|r| !removed.contains(&r.id)) {
        heads.entry(rev.key.clone()).or_default().push(rev);
    }
    Ok((heads, fingerprint, count))
}
fn winner(revs: &[Revision]) -> Option<&Revision> {
    // Keep edits visible when an offline deletion races an edit. Explicit resolution can delete.
    revs.iter()
        .rev()
        .find(|r| r.value.is_some())
        .or_else(|| revs.last())
}
pub fn records(workspace: &Workspace) -> Result<Records, String> {
    if workspace.version != 1 || !matches!(workspace.theme.as_str(), "dark" | "light" | "system") {
        return Err("工作空间配置格式不受支持。".into());
    }
    let mut result = Records::new();
    for chat in &workspace.conversations {
        let key = format!("chat/{}", chat.id);
        if !valid_key(&key) || result.contains_key(&key) {
            return Err("会话 ID 无效或重复。".into());
        }
        result.insert(
            key,
            serde_json::to_value(chat).map_err(|_| "无法序列化会话。")?,
        );
    }
    for connection in &workspace.connections {
        let key = format!("connection/{}", connection.id);
        if !valid_key(&key) || result.contains_key(&key) {
            return Err("连接 ID 无效或重复。".into());
        }
        let value = serde_json::to_value(connection).map_err(|_| "无法序列化连接。")?;
        result.insert(key, value);
    }
    if let Some(prompts) = &workspace.prompts {
        for prompt in prompts {
            let key = format!("prompt/{}", prompt.id);
            if !valid_key(&key) || result.contains_key(&key) {
                return Err("提示词 ID 无效或重复。".into());
            }
            result.insert(
                key,
                serde_json::to_value(prompt).map_err(|_| "无法序列化提示词。")?,
            );
        }
    }
    if let Some(generation) = &workspace.generation {
        generation.validate()?;
        result.insert(
            "preference/generation".into(),
            serde_json::to_value(generation).unwrap(),
        );
    }
    if let Some(id) = &workspace.default_prompt_id {
        result.insert("preference/defaultPromptId".into(), json!(id));
    }
    result.insert("preference/theme".into(), json!(workspace.theme));
    result.insert(
        "preference/defaultConnectionId".into(),
        json!(workspace.default_connection_id),
    );
    result.insert(
        "preference/defaultModel".into(),
        json!(workspace.default_model),
    );
    let images = serde_json::to_value(&workspace.image_settings).unwrap();
    result.insert("preference/imageSettings".into(), images);
    Ok(result)
}
fn materialize(heads: &Heads, active: Option<String>) -> Result<Workspace, String> {
    let mut value = json!({"version":1,"connections":[],"conversations":[],"activeConversationId":active,"defaultConnectionId":"","defaultModel":"","theme":"light","imageSettings":{"mode":"base64","hasUploadConfig":false}});
    for (key, revs) in heads {
        let Some(item) = winner(revs).and_then(|r| r.value.clone()) else {
            continue;
        };
        if let Some(id) = key.strip_prefix("chat/") {
            if item["id"] != id {
                return Err("会话 ID 与记录不一致。".into());
            }
            value["conversations"].as_array_mut().unwrap().push(item);
        } else if let Some(id) = key.strip_prefix("connection/") {
            if item["id"] != id {
                return Err("连接 ID 与记录不一致。".into());
            }
            value["connections"].as_array_mut().unwrap().push(item);
        } else if let Some(id) = key.strip_prefix("prompt/") {
            if item["id"] != id {
                return Err("提示词 ID 与记录不一致。".into());
            }
            if value.get("prompts").is_none() {
                value["prompts"] = json!([]);
            }
            value["prompts"].as_array_mut().unwrap().push(item);
        } else if let Some(name) = key.strip_prefix("preference/") {
            value[name] = item;
        }
    }
    if (heads.contains_key("preference/defaultPromptId")
        || heads.keys().any(|key| key.starts_with("prompt/")))
        && value.get("prompts").is_none()
    {
        value["prompts"] = json!([]);
    }
    let mut workspace: Workspace =
        serde_json::from_value(value).map_err(|_| "同步数据格式不受支持，已保留所有版本。")?;
    // Recovery changes the display baseline without publishing a new edit on reading clients.
    for chat in &mut workspace.conversations {
        for message in &mut chat.messages {
            if message.status.as_deref() == Some("streaming") {
                message.status = Some("stopped".into());
            }
            for step in &mut message.agent_steps {
                if step.status != "complete" {
                    step.status = "stopped".into();
                }
            }
        }
        for image in &mut chat.draft_images {
            if matches!(image.status.as_deref(), Some("uploading" | "loading")) {
                image.status = Some("error".into());
                image.error = Some("图片处理已中断，请重试或移除。".into());
            }
        }
    }
    records(&workspace)?;
    workspace
        .conversations
        .sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    Ok(workspace)
}
fn write_event(directory: &Path, revisions: Vec<Revision>) -> Result<(), String> {
    if revisions.is_empty() {
        return Ok(());
    }
    let bytes = serde_json::to_vec(&Event {
        version: 1,
        revisions,
    })
    .map_err(|_| "无法编码同步记录。")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("单次变更超过 32 MB，未保存。".into());
    }
    atomic(
        &directory
            .join("sync-v1/events")
            .join(format!("{}.json", uuid::Uuid::new_v4())),
        &bytes,
        true,
    )
}
impl Session {
    #[cfg(test)]
    pub fn init(&mut self, local: &Path) -> Result<(), String> {
        self.init_with_default(local, local)
    }
    pub fn init_with_default(
        &mut self,
        local: &Path,
        default_directory: &Path,
    ) -> Result<(), String> {
        if self.local.is_some() {
            return Ok(());
        }
        let path = local.join("storage-location.json");
        let location: Location = if path.exists() {
            read_json(&path)?
        } else {
            Location {
                version: 1,
                directory: None,
                device: uuid::Uuid::new_v4().to_string(),
                active_conversation_id: None,
            }
        };
        if location.version != 1 || uuid::Uuid::parse_str(&location.device).is_err() {
            return Err("本机存储位置配置无效。".into());
        }
        let was_default = location.directory.as_deref().is_none_or(|directory| {
            directory == local
                || fs::canonicalize(directory)
                    .ok()
                    .zip(fs::canonicalize(local).ok())
                    .is_some_and(|(a, b)| a == b)
        });
        self.local = Some(local.to_owned());
        self.default_directory = Some(default_directory.to_owned());
        self.location = Some(location);
        if was_default && local != default_directory {
            // Convert legacy data before copying immutable revisions and images. Keep the source.
            if local.join("workspace.json").exists() || local.join("sync-v1/events").exists() {
                self.location.as_mut().unwrap().directory = Some(local.to_owned());
                self.load()?;
                self.switch(
                    default_directory.to_str().ok_or("默认目录路径无效。")?,
                    true,
                )?;
            } else {
                fs::create_dir_all(default_directory)
                    .map_err(|e| format!("无法创建默认数据目录：{e}"))?;
                self.location.as_mut().unwrap().directory = Some(default_directory.to_owned());
            }
        }
        self.persist_location()?;
        Ok(())
    }
    fn persist_location(&self) -> Result<(), String> {
        atomic(
            &self
                .local
                .as_ref()
                .ok_or("存储尚未初始化。")?
                .join("storage-location.json"),
            &serde_json::to_vec(self.location.as_ref().unwrap()).unwrap(),
            false,
        )
    }
    pub fn directory(&self) -> Result<PathBuf, String> {
        Ok(self
            .location
            .as_ref()
            .ok_or("存储尚未初始化。")?
            .directory
            .clone()
            .unwrap_or_else(|| self.default_directory.clone().unwrap()))
    }
    pub fn device(&self) -> &str {
        &self.location.as_ref().unwrap().device
    }
    pub fn load(&mut self) -> Result<Option<Workspace>, String> {
        let dir = self.directory()?;
        if self.location.as_ref().unwrap().directory.is_some() && !dir.is_dir() {
            return Err(
                "配置的数据目录暂不可用。请等待同步盘挂载，或在存储设置中切换目录。".into(),
            );
        }
        let (mut heads, _, _) = scan(&dir)?;
        if heads.is_empty()
            && let Some(legacy) = crate::storage::load(&dir)?
        {
            let changes = records(&legacy)?
                .into_iter()
                .map(|(key, value)| Revision {
                    id: uuid::Uuid::new_v4().to_string(),
                    device: self.device().into(),
                    key,
                    supersedes: vec![],
                    value: Some(value),
                })
                .collect();
            write_event(&dir, changes)?;
            heads = scan(&dir)?.0;
            if self
                .location
                .as_ref()
                .unwrap()
                .active_conversation_id
                .is_none()
            {
                self.location.as_mut().unwrap().active_conversation_id =
                    legacy.active_conversation_id;
                self.persist_location()?;
            }
        }
        let (_, fingerprint, _) = scan(&dir)?;
        let workspace = materialize(
            &heads,
            self.location
                .as_ref()
                .unwrap()
                .active_conversation_id
                .clone(),
        )?;
        self.baseline = records(&workspace)?;
        self.heads = heads;
        self.fingerprint = fingerprint;
        Ok(Some(workspace))
    }
    pub fn save(&mut self, workspace: &Workspace) -> Result<(), String> {
        let dir = self.directory()?;
        scan(&dir)?; // Validate incoming remote data without pretending the UI has observed it.
        let incoming = records(workspace)?;
        let keys: BTreeSet<_> = self
            .baseline
            .keys()
            .chain(incoming.keys())
            .cloned()
            .collect();
        let mut revisions = Vec::new();
        for key in keys {
            if self.baseline.get(&key) == incoming.get(&key) {
                continue;
            }
            revisions.push(Revision {
                id: uuid::Uuid::new_v4().to_string(),
                device: self.device().into(),
                supersedes: self
                    .heads
                    .get(&key)
                    .map(|r| r.iter().map(|r| r.id.clone()).collect())
                    .unwrap_or_default(),
                value: incoming.get(&key).cloned(),
                key,
            });
        }
        write_event(&dir, revisions.clone())?;
        for rev in revisions {
            self.heads.insert(rev.key.clone(), vec![rev]);
        }
        self.baseline = incoming;
        // Include only observed revisions plus our writes. Remote IDs remain pending until load().
        let observed: BTreeSet<_> = self
            .fingerprint
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .chain(self.heads.values().flatten().map(|r| r.id.clone()))
            .collect();
        self.fingerprint = observed.into_iter().collect::<Vec<_>>().join(",");
        if self.location.as_ref().unwrap().active_conversation_id
            != workspace.active_conversation_id
        {
            self.location.as_mut().unwrap().active_conversation_id =
                workspace.active_conversation_id.clone();
            self.persist_location()?;
        }
        Ok(())
    }
    pub fn refresh_secrets(&mut self) -> Result<(), String> {
        let (heads, _, _) = scan(&self.directory()?)?;
        self.heads.retain(|key, _| !key.starts_with("secret/"));
        self.heads.extend(
            heads
                .into_iter()
                .filter(|(key, _)| key.starts_with("secret/")),
        );
        Ok(())
    }
    pub fn has_record(&self, key: &str) -> bool {
        self.heads.contains_key(key)
    }
    pub fn secret(&self, key: &str) -> Option<Value> {
        self.heads
            .get(key)
            .and_then(|r| winner(r))
            .and_then(|r| r.value.clone())
    }
    pub fn secret_keys(&self) -> Vec<String> {
        self.heads
            .keys()
            .filter(|key| key.starts_with("secret/"))
            .cloned()
            .collect()
    }
    pub fn write_secret(&mut self, key: &str, value: Option<Value>) -> Result<(), String> {
        self.write_secrets(vec![(key.to_owned(), value)])
    }
    /// One immutable event commits a batch, preserving concurrent heads from other devices.
    pub fn write_secrets(&mut self, values: Vec<(String, Option<Value>)>) -> Result<(), String> {
        let mut revisions = Vec::new();
        let mut keys = BTreeSet::new();
        for (key, value) in values {
            if !valid_key(&key) || !key.starts_with("secret/") || !keys.insert(key.clone()) {
                return Err("无效或重复的凭据记录。".into());
            }
            revisions.push(Revision {
                id: uuid::Uuid::new_v4().to_string(),
                device: self.device().into(),
                supersedes: self
                    .heads
                    .get(&key)
                    .map(|r| r.iter().map(|r| r.id.clone()).collect())
                    .unwrap_or_default(),
                key,
                value,
            });
        }
        write_event(&self.directory()?, revisions.clone())?;
        for rev in revisions {
            self.heads.insert(rev.key.clone(), vec![rev]);
        }
        Ok(())
    }
    pub fn status(&self) -> Result<Status, String> {
        let (heads, fingerprint, count) = scan(&self.directory()?)?;
        let conflicts = heads
            .iter()
            .filter(|(_, revs)| {
                let values: BTreeSet<_> = revs
                    .iter()
                    .map(|r| serde_json::to_string(&r.value).unwrap())
                    .collect();
                values.len() > 1
            })
            .map(|(key, revs)| Conflict {
                key: key.clone(),
                label: if key.starts_with("secret/") {
                    format!("凭据：{}", key.strip_prefix("secret/").unwrap())
                } else {
                    winner(revs)
                        .and_then(|r| r.value.as_ref())
                        .and_then(|v| v["title"].as_str().or(v["name"].as_str()))
                        .unwrap_or(key)
                        .into()
                },
                variants: revs
                    .iter()
                    .map(|r| Variant {
                        revision: r.id.clone(),
                        device: r.device.clone(),
                        deleted: r.value.is_none(),
                        preview: if key.starts_with("secret/") {
                            if r.value.is_some() {
                                "凭据版本（内容隐藏）"
                            } else {
                                "删除标记"
                            }
                            .into()
                        } else {
                            r.value
                                .as_ref()
                                .map(|v| serde_json::to_string_pretty(v).unwrap())
                                .unwrap_or_else(|| "删除标记".into())
                        },
                    })
                    .collect(),
            })
            .collect();
        Ok(Status {
            directory: self.directory()?.display().to_string(),
            default_directory: self
                .default_directory
                .as_ref()
                .unwrap()
                .display()
                .to_string(),
            device: self.device().into(),
            pending: fingerprint != self.fingerprint,
            conflicts,
            revision_count: count,
        })
    }
    pub fn resolve(
        &mut self,
        key: &str,
        revision: &str,
        keep_both: bool,
    ) -> Result<Vec<String>, String> {
        let (heads, _, _) = scan(&self.directory()?)?;
        let versions = heads.get(key).ok_or("冲突已变化，请刷新后重试。")?;
        let selected = versions
            .iter()
            .find(|r| r.id == revision)
            .ok_or("该版本已变化，请刷新后重试。")?;
        let mut changes = vec![Revision {
            id: uuid::Uuid::new_v4().to_string(),
            device: self.device().into(),
            key: key.into(),
            supersedes: versions.iter().map(|r| r.id.clone()).collect(),
            value: selected.value.clone(),
        }];
        let mut connection_copies = Vec::new();
        if keep_both {
            if !key.starts_with("chat/")
                && !key.starts_with("connection/")
                && !key.starts_with("prompt/")
            {
                return Err("只有会话、模型连接和提示词可保留为独立副本。".into());
            }
            let mut values = BTreeSet::new();
            values.insert(serde_json::to_string(&selected.value).unwrap());
            for version in versions {
                let Some(mut value) = version.value.clone() else {
                    continue;
                };
                if !values.insert(serde_json::to_string(&version.value).unwrap()) {
                    continue;
                }
                let id = uuid::Uuid::new_v4().to_string();
                value["id"] = json!(id);
                let field = if key.starts_with("chat/") {
                    "title"
                } else {
                    "name"
                };
                value[field] = json!(format!(
                    "{}（同步冲突副本）",
                    value[field].as_str().unwrap_or("未命名")
                ));
                if key.starts_with("connection/") {
                    connection_copies.push(id.clone());
                }
                changes.push(Revision {
                    id: uuid::Uuid::new_v4().to_string(),
                    device: self.device().into(),
                    key: format!("{}/{}", key.split('/').next().unwrap(), id),
                    supersedes: vec![],
                    value: Some(value),
                });
            }
        }
        write_event(&self.directory()?, changes)?;
        Ok(connection_copies)
    }
    pub fn switch(&mut self, target: &str, migrate: bool) -> Result<(), String> {
        let old = self.directory()?;
        let target = if target.trim().is_empty() {
            self.default_directory.clone().unwrap()
        } else {
            PathBuf::from(target.trim())
        };
        if !target.is_absolute() {
            return Err("请输入当前设备上的绝对目录路径。".into());
        }
        fs::create_dir_all(&target).map_err(|e| format!("无法创建目标目录：{e}"))?;
        let target = fs::canonicalize(target).map_err(|_| "无法解析目标目录。")?;
        let old = if old.exists() {
            fs::canonicalize(&old).map_err(|_| "无法解析当前目录。")?
        } else if migrate {
            return Err("当前目录不可用。取消迁移可直接打开目标目录；原目录会保留。".into());
        } else {
            old
        };
        if old == target {
            return Ok(());
        }
        if target.starts_with(&old) || old.starts_with(&target) {
            return Err("目标目录不能与当前目录互相包含，请选择独立的数据文件夹。".into());
        }
        if migrate {
            let (source_heads, _, _) = scan(&old)?;
            if source_heads.is_empty() {
                crate::storage::load(&old)?;
            }
        }
        // Seed a pre-existing legacy target without overwriting its data.
        let (target_heads, _, _) = scan(&target)?;
        if target_heads.is_empty()
            && let Some(legacy) = crate::storage::load(&target)?
        {
            write_event(
                &target,
                records(&legacy)?
                    .into_iter()
                    .map(|(key, value)| Revision {
                        id: uuid::Uuid::new_v4().to_string(),
                        device: self.device().into(),
                        key,
                        supersedes: vec![],
                        value: Some(value),
                    })
                    .collect(),
            )?;
        }
        for sub in if migrate {
            vec!["images", "sync-v1/events"]
        } else {
            vec![]
        } {
            let source = old.join(sub);
            if !source.exists() {
                continue;
            }
            for entry in fs::read_dir(&source).map_err(|e| format!("无法读取原目录：{e}"))?
            {
                let entry = entry.map_err(|_| "无法读取迁移文件。")?;
                let path = entry.path();
                if sub == "images"
                    && (path.extension().and_then(|s| s.to_str()) != Some("bin")
                        || path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())
                            .is_none())
                {
                    continue;
                }
                if sub != "images" && path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let meta = fs::symlink_metadata(&path).map_err(|_| "迁移文件不可读。")?;
                if !meta.is_file() || meta.len() > MAX_FILE {
                    return Err("迁移文件不是普通文件或过大。".into());
                }
                let dest = target.join(sub).join(entry.file_name());
                let bytes = fs::read(&path).map_err(|_| "无法读取迁移文件。")?;
                if dest.exists() {
                    if fs::read(&dest).map_err(|_| "无法读取目标文件。")? != bytes {
                        return Err(
                            "目标目录有同名不同内容的文件，已停止切换，原目录仍在使用。".into()
                        );
                    }
                } else {
                    atomic(&dest, &bytes, true)?;
                }
            }
        }
        let (heads, _, _) = scan(&target)?;
        materialize(&heads, None)?;
        let previous = self.location.as_ref().unwrap().directory.clone();
        self.location.as_mut().unwrap().directory = Some(target);
        if let Err(error) = self.persist_location() {
            self.location.as_mut().unwrap().directory = previous;
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn blank() -> Workspace {
        serde_json::from_value(json!({"version":1,"connections":[],"conversations":[],"activeConversationId":null,"defaultConnectionId":"","defaultModel":"","theme":"dark"})).unwrap()
    }
    fn chat(id: &str, title: &str) -> crate::models::Conversation {
        serde_json::from_value(json!({"id":id,"title":title,"messages":[],"createdAt":"2026-01-01","updatedAt":"2026-01-01","pinned":false,"connectionId":"","model":"","systemPrompt":"","maxTokens":4096,"temperature":null})).unwrap()
    }
    fn start(local: &Path, shared: &Path) -> Session {
        let mut s = Session::default();
        s.init(local).unwrap();
        s.switch(shared.to_str().unwrap(), false).unwrap();
        s.load().unwrap();
        s
    }
    fn replicate(source: &Path, target: &Path) {
        let source = source.join("sync-v1/events");
        if !source.exists() {
            return;
        }
        for e in fs::read_dir(source).unwrap() {
            let e = e.unwrap();
            let dest = target.join("sync-v1/events").join(e.file_name());
            if !dest.exists() {
                atomic(&dest, &fs::read(e.path()).unwrap(), true).unwrap();
            }
        }
    }
    #[test]
    fn prompts_and_global_generation_sync_independently_and_keep_conflict_copies() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let mut a = start(&root.path().join("device-a"), &shared);
        let mut initial = blank();
        initial.prompts = Some(vec![crate::models::Prompt {
            id: "prompt-one".into(),
            name: "Editor".into(),
            content: "Original".into(),
        }]);
        initial.default_prompt_id = Some("prompt-one".into());
        initial.generation = Some(Default::default());
        a.save(&initial).unwrap();
        let mut b = start(&root.path().join("device-b"), &shared);
        let mut wa = a.load().unwrap().unwrap();
        let mut wb = b.load().unwrap().unwrap();
        wa.prompts.as_mut().unwrap()[0].content = "Left".into();
        wb.generation.as_mut().unwrap().max_tokens = 8192;
        a.save(&wa).unwrap();
        b.save(&wb).unwrap();
        let merged = a.load().unwrap().unwrap();
        assert_eq!(merged.prompts.as_ref().unwrap()[0].content, "Left");
        assert_eq!(merged.generation.as_ref().unwrap().max_tokens, 8192);
        assert!(a.status().unwrap().conflicts.is_empty());
        wa = merged;
        wb = b.load().unwrap().unwrap();
        wa.prompts.as_mut().unwrap()[0].content = "Branch A".into();
        wb.prompts.as_mut().unwrap()[0].content = "Branch B".into();
        a.save(&wa).unwrap();
        b.save(&wb).unwrap();
        let status = a.status().unwrap();
        assert_eq!(status.conflicts.len(), 1);
        assert_eq!(status.conflicts[0].key, "prompt/prompt-one");
        a.resolve(
            "prompt/prompt-one",
            &status.conflicts[0].variants[0].revision,
            true,
        )
        .unwrap();
        let mut merged = a.load().unwrap().unwrap();
        assert_eq!(merged.prompts.as_ref().unwrap().len(), 2);
        assert!(a.status().unwrap().conflicts.is_empty());
        merged.prompts = Some(vec![]);
        merged.default_prompt_id = Some(String::new());
        a.save(&merged).unwrap();
        assert!(a.load().unwrap().unwrap().prompts.unwrap().is_empty());
    }
    #[test]
    fn offline_clients_merge_independent_records_and_converge() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        let mut first = start(&root.path().join("device-a"), &a);
        let mut second = start(&root.path().join("device-b"), &b);
        let mut wa = blank();
        wa.conversations.push(chat("one", "A"));
        first.save(&wa).unwrap();
        let mut wb = blank();
        wb.conversations.push(chat("two", "B"));
        second.save(&wb).unwrap();
        replicate(&a, &b);
        replicate(&b, &a);
        assert!(first.status().unwrap().pending);
        assert!(second.status().unwrap().pending);
        let wa = first.load().unwrap().unwrap();
        let wb = second.load().unwrap().unwrap();
        assert_eq!(wa.conversations.len(), 2);
        assert_eq!(
            serde_json::to_value(wa).unwrap(),
            serde_json::to_value(wb).unwrap()
        );
        assert!(first.status().unwrap().conflicts.is_empty());
    }
    #[test]
    fn concurrent_chat_edits_are_preserved_and_can_become_independent_copies() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let mut first = start(&root.path().join("device-a"), &shared);
        let mut original = blank();
        original.conversations.push(chat("one", "original"));
        first.save(&original).unwrap();
        let mut second = start(&root.path().join("device-b"), &shared);
        let mut left = first.load().unwrap().unwrap();
        let mut right = second.load().unwrap().unwrap();
        left.conversations[0].title = "left".into();
        right.conversations[0].title = "right".into();
        first.save(&left).unwrap();
        second.save(&right).unwrap();
        // A stale follow-up save must not silently acknowledge the other branch.
        right.conversations[0].draft = "continued offline".into();
        second.save(&right).unwrap();
        let status = first.status().unwrap();
        assert_eq!(status.conflicts.len(), 1);
        assert_eq!(status.conflicts[0].variants.len(), 2);
        let revision = &status.conflicts[0].variants[0].revision;
        first.resolve("chat/one", revision, true).unwrap();
        let result = first.load().unwrap().unwrap();
        assert_eq!(result.conversations.len(), 2);
        assert!(
            result
                .conversations
                .iter()
                .any(|c| c.title.contains("同步冲突副本"))
        );
        assert!(first.status().unwrap().conflicts.is_empty());
    }
    #[test]
    fn tombstones_prevent_resurrection_but_keep_concurrent_edits() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let mut a = start(&root.path().join("device-a"), &shared);
        let mut workspace = blank();
        workspace.conversations.push(chat("one", "original"));
        a.save(&workspace).unwrap();
        let mut b = start(&root.path().join("device-b"), &shared);
        let mut stale = b.load().unwrap().unwrap();
        a.load().unwrap();
        a.save(&blank()).unwrap();
        b.save(&stale).unwrap();
        assert!(a.load().unwrap().unwrap().conversations.is_empty());
        stale.conversations[0].title = "offline edit".into();
        b.save(&stale).unwrap();
        let restored = a.load().unwrap().unwrap();
        assert_eq!(restored.conversations[0].title, "offline edit");
        assert_eq!(a.status().unwrap().conflicts.len(), 1);
        let status = a.status().unwrap();
        let deleted = status.conflicts[0]
            .variants
            .iter()
            .find(|v| v.deleted)
            .unwrap();
        a.resolve("chat/one", &deleted.revision, false).unwrap();
        assert!(a.load().unwrap().unwrap().conversations.is_empty());
    }
    #[test]
    fn preferences_merge_by_field_and_same_field_conflicts_are_explicit() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let mut a = start(&root.path().join("device-a"), &shared);
        a.save(&blank()).unwrap();
        let mut b = start(&root.path().join("device-b"), &shared);
        let mut wa = a.load().unwrap().unwrap();
        let mut wb = b.load().unwrap().unwrap();
        wa.theme = "light".into();
        wb.image_settings.mode = "url".into();
        a.save(&wa).unwrap();
        b.save(&wb).unwrap();
        let merged = a.load().unwrap().unwrap();
        assert_eq!(merged.theme, "light");
        assert_eq!(merged.image_settings.mode, "url");
        assert!(a.status().unwrap().conflicts.is_empty());
        b.load().unwrap();
        wa = merged.clone();
        wb = merged;
        wa.theme = "dark".into();
        wb.theme = "system".into();
        a.save(&wa).unwrap();
        b.save(&wb).unwrap();
        assert_eq!(a.status().unwrap().conflicts[0].key, "preference/theme");
    }
    #[test]
    fn sync_software_conflict_copies_are_deduplicated_and_corruption_blocks_writes() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Session::default();
        s.init(root.path()).unwrap();
        s.load().unwrap();
        let mut w = blank();
        w.theme = "dark".into();
        s.save(&w).unwrap();
        let events = root.path().join("sync-v1/events");
        let first = fs::read_dir(&events)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::copy(&first, events.join("conflicted copy.json")).unwrap();
        assert_eq!(s.status().unwrap().revision_count, 1);
        assert!(s.status().unwrap().conflicts.is_empty());
        fs::write(events.join("broken.json"), "{").unwrap();
        assert!(s.save(&blank()).is_err());
        assert_eq!(fs::read_to_string(events.join("broken.json")).unwrap(), "{");
    }
    #[test]
    fn migrates_legacy_and_merges_target_without_removing_source_images() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("device");
        fs::create_dir_all(&local).unwrap();
        let mut old = blank();
        old.conversations.push(chat("one", "local"));
        crate::storage::save(&local, &old).unwrap();
        let image_id = uuid::Uuid::new_v4().to_string();
        atomic(
            &local.join("images").join(format!("{image_id}.bin")),
            b"image",
            true,
        )
        .unwrap();
        let target = root.path().join("synced");
        fs::create_dir_all(&target).unwrap();
        let mut other = blank();
        other.conversations.push(chat("two", "remote"));
        crate::storage::save(&target, &other).unwrap();
        let mut s = Session::default();
        s.init(&local).unwrap();
        s.load().unwrap();
        s.switch(target.to_str().unwrap(), true).unwrap();
        assert_eq!(s.load().unwrap().unwrap().conversations.len(), 2);
        assert!(local.join("workspace.json").exists());
        assert!(
            target
                .join("images")
                .join(format!("{image_id}.bin"))
                .exists()
        );
        let mut reopened = Session::default();
        reopened.init(&local).unwrap();
        assert_eq!(
            reopened.directory().unwrap(),
            fs::canonicalize(target).unwrap()
        );
        assert!(
            reopened
                .switch(local.join("child").to_str().unwrap(), true)
                .is_ok()
        ); // unrelated to the current synced folder
    }
    #[test]
    fn unavailable_directory_is_not_silently_recreated_and_can_be_switched() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("unmounted");
        let mut s = start(&root.path().join("local"), &shared);
        fs::remove_dir_all(&shared).unwrap();
        let mut w = blank();
        w.theme = "light".into();
        assert!(s.save(&w).is_err());
        assert!(!shared.exists());
        let next = root.path().join("another-folder");
        s.switch(next.to_str().unwrap(), false).unwrap();
        assert!(s.load().is_ok());
        assert!(s.switch(root.path().to_str().unwrap(), true).is_err());
    }
    #[test]
    fn encrypted_credentials_preserve_concurrent_versions_without_previewing_secrets() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let mut a = start(&root.path().join("a"), &shared);
        let mut b = start(&root.path().join("b"), &shared);
        let left =
            serde_json::to_value(crate::vault::seal(b"private-key-one", "sync-password").unwrap())
                .unwrap();
        let right =
            serde_json::to_value(crate::vault::seal(b"private-key-two", "sync-password").unwrap())
                .unwrap();
        a.write_secret("secret/connection-id", Some(left)).unwrap();
        b.write_secret("secret/connection-id", Some(right)).unwrap();
        let status = a.status().unwrap();
        assert_eq!(status.conflicts.len(), 1);
        let rendered = serde_json::to_string(&status).unwrap();
        assert!(!rendered.contains("private-key"));
        assert!(!rendered.contains("ciphertext"));
        a.resolve(
            "secret/connection-id",
            &status.conflicts[0].variants[0].revision,
            false,
        )
        .unwrap();
        a.load().unwrap();
        let sealed = serde_json::from_value(a.secret("secret/connection-id").unwrap()).unwrap();
        assert!(crate::vault::open(&sealed, "sync-password").is_ok());
        assert!(a.status().unwrap().conflicts.is_empty());
    }
    #[test]
    fn recovery_display_does_not_create_edits_on_reading_clients() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        let mut a = start(&root.path().join("a"), &shared);
        let mut w = blank();
        let mut c = chat("one", "partial");
        c.messages.push(serde_json::from_value(json!({"id":"reply","role":"assistant","content":"partial reply","createdAt":"now","status":"streaming","agentSteps":[{"number":1,"protocol":"responses","content":"","reasoning":"","calls":[{"id":"call","name":"search","arguments":{}}],"results":[],"status":"tools"}]})).unwrap());
        w.conversations.push(c);
        a.save(&w).unwrap();
        let mut b = start(&root.path().join("b"), &shared);
        let loaded = b.load().unwrap().unwrap();
        assert_eq!(
            loaded.conversations[0].messages[0].status.as_deref(),
            Some("stopped")
        );
        assert_eq!(
            loaded.conversations[0].messages[0].agent_steps[0].status,
            "stopped"
        );
        let count = b.status().unwrap().revision_count;
        b.save(&loaded).unwrap();
        assert_eq!(b.status().unwrap().revision_count, count);
        assert!(b.status().unwrap().conflicts.is_empty());
    }
    #[test]
    fn new_install_uses_home_default_and_keeps_device_metadata_separate() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("app-data");
        let default = root.path().join("home/.oh-my-chatwise");
        let mut s = Session::default();
        s.init_with_default(&local, &default).unwrap();
        assert_eq!(s.directory().unwrap(), default);
        assert!(default.is_dir());
        assert!(local.join("storage-location.json").is_file());
        assert_eq!(
            s.status().unwrap().default_directory,
            default.display().to_string()
        );
    }
    #[test]
    fn old_default_migrates_to_home_once_and_preserves_original_data() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("app-data");
        let default = root.path().join("home/.oh-my-chatwise");
        let mut old = Session::default();
        old.init(&local).unwrap();
        old.load().unwrap();
        let mut w = blank();
        w.conversations.push(chat("old-chat", "existing"));
        old.save(&w).unwrap();
        let image = uuid::Uuid::new_v4().to_string();
        atomic(
            &local.join("images").join(format!("{image}.bin")),
            b"image",
            true,
        )
        .unwrap();
        let mut upgraded = Session::default();
        upgraded.init_with_default(&local, &default).unwrap();
        assert_eq!(upgraded.load().unwrap().unwrap().conversations.len(), 1);
        assert!(local.join("sync-v1/events").is_dir());
        assert!(
            default
                .join("images")
                .join(format!("{image}.bin"))
                .is_file()
        );
        let count = upgraded.status().unwrap().revision_count;
        let mut restarted = Session::default();
        restarted.init_with_default(&local, &default).unwrap();
        assert_eq!(restarted.status().unwrap().revision_count, count);
    }
    #[test]
    fn custom_directory_is_preserved_and_reset_uses_new_home_default() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("app-data");
        let custom = root.path().join("onedrive");
        let default = root.path().join("home/.oh-my-chatwise");
        let mut old = start(&local, &custom);
        let mut w = blank();
        w.theme = "light".into();
        old.save(&w).unwrap();
        let mut upgraded = Session::default();
        upgraded.init_with_default(&local, &default).unwrap();
        assert_eq!(
            upgraded.directory().unwrap(),
            fs::canonicalize(custom).unwrap()
        );
        upgraded.switch("", true).unwrap();
        assert_eq!(
            upgraded.directory().unwrap(),
            fs::canonicalize(default).unwrap()
        );
        assert_eq!(upgraded.load().unwrap().unwrap().theme, "light");
    }
    #[test]
    fn per_device_focus_does_not_create_shared_changes() {
        let root = tempfile::tempdir().unwrap();
        let mut s = Session::default();
        s.init(root.path()).unwrap();
        let mut w = s.load().unwrap().unwrap();
        w.active_conversation_id = Some("one".into());
        s.save(&w).unwrap();
        assert_eq!(s.status().unwrap().revision_count, 0);
        assert_eq!(
            s.load().unwrap().unwrap().active_conversation_id.as_deref(),
            Some("one")
        );
    }
}
