use crate::models::Workspace;
#[cfg(test)]
use std::io::Write;
use std::{fs, path::Path};

const LIMIT: u64 = 32 * 1024 * 1024;
pub fn load(directory: &Path) -> Result<Option<Workspace>, String> {
    let path = directory.join("workspace.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取本地会话：{error}")),
    };
    if bytes.len() as u64 > LIMIT {
        return Err("会话文件超过 32 MB，未修改原文件。".into());
    }
    let workspace: Workspace = serde_json::from_slice(&bytes)
        .map_err(|_| "会话文件格式有误，原文件与 workspace.backup.json 已保留。".to_string())?;
    if workspace.version != 1 {
        return Err("暂不支持此版本的会话文件，未修改原文件。".into());
    }
    Ok(Some(workspace))
}
#[cfg(test)]
pub fn save(directory: &Path, workspace: &Workspace) -> Result<(), String> {
    if workspace.version != 1 {
        return Err("不支持的工作空间版本。".into());
    }
    let bytes = serde_json::to_vec(workspace).map_err(|_| "无法序列化会话。".to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("会话数据超过 32 MB，请先导出并清理部分对话。".into());
    }
    fs::create_dir_all(directory).map_err(|error| format!("无法创建数据目录：{error}"))?;
    let path = directory.join("workspace.json");
    // Refuse to overwrite corrupted or future-version data, even if called directly.
    if path.exists() {
        load(directory)?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| format!("无法创建临时会话文件：{error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("无法写入本地会话：{error}"))?;
    if path.exists() {
        fs::copy(&path, directory.join("workspace.backup.json"))
            .map_err(|error| format!("无法备份会话：{error}"))?;
    }
    temporary
        .persist(&path)
        .map_err(|error| format!("无法替换会话文件：{error}"))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> Workspace {
        serde_json::from_value(serde_json::json!({"version":1,"connections":[],"conversations":[],"activeConversationId":null,"defaultConnectionId":"","defaultModel":"","theme":"dark"})).unwrap()
    }
    #[test]
    fn roundtrip_backup_and_corruption_protection() {
        let directory = tempfile::tempdir().unwrap();
        assert!(load(directory.path()).unwrap().is_none());
        save(directory.path(), &workspace()).unwrap();
        let mut changed = workspace();
        changed.theme = "light".into();
        save(directory.path(), &changed).unwrap();
        assert_eq!(load(directory.path()).unwrap().unwrap().theme, "light");
        let backup: Workspace = serde_json::from_slice(
            &fs::read(directory.path().join("workspace.backup.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(backup.theme, "dark");
        fs::write(directory.path().join("workspace.json"), "broken").unwrap();
        assert!(save(directory.path(), &workspace()).is_err());
        assert_eq!(
            fs::read_to_string(directory.path().join("workspace.json")).unwrap(),
            "broken"
        );
    }
    #[test]
    fn unknown_fields_cannot_persist_secrets() {
        let mut value = serde_json::to_value(workspace()).unwrap();
        value["apiKey"] = "do-not-save".into();
        let decoded: Workspace = serde_json::from_value(value).unwrap();
        assert!(
            !serde_json::to_string(&decoded)
                .unwrap()
                .contains("do-not-save")
        );
    }
}
