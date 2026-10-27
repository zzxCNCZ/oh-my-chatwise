//! Local-only upgrade snapshot and optional sync password. Never accesses OS credentials.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
const MAX_FILE: u64 = 16 * 1024 * 1024;
const PASSWORD_KEY: &str = "storage-sync-password";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    entries: BTreeMap<String, String>,
}
struct Store {
    directory: PathBuf,
}
static STORES: OnceLock<Mutex<BTreeMap<String, Store>>> = OnceLock::new();
fn stores() -> &'static Mutex<BTreeMap<String, Store>> {
    STORES.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 100 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        Err("无效的凭据 ID。".into())
    } else {
        Ok(())
    }
}
fn check_regular(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_FILE => Ok(true),
        Ok(_) => Err("凭据文件不是普通文件或超过大小限制，已停止读写。".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("无法检查本地凭据文件。".into()),
    }
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    if !check_regular(path)? {
        return Ok(None);
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| "无法读取本地凭据文件。")?)
        .map(Some)
        .map_err(|_| "本地凭据文件损坏或格式不受支持，未覆盖原文件。".into())
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    check_regular(path)?;
    let parent = path.parent().ok_or("凭据目录无效。")?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "无法编码凭据文件。")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("凭据文件超过 16 MB。".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|_| "无法创建凭据临时文件。")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| "无法设置凭据文件权限。")?;
    }
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| "无法写入凭据文件。")?;
    temp.persist(path)
        .map_err(|_| "无法提交凭据文件，原文件已保留。")?;
    Ok(())
}
fn lock(directory: &Path) -> Result<File, String> {
    let path = directory.join("credentials.lock");
    check_regular(&path)?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|_| "无法打开凭据锁。")?;
    file.lock().map_err(|_| "无法锁定凭据文件。")?;
    Ok(file)
}

impl Store {
    fn document(&self) -> Result<Document, String> {
        let doc: Document =
            read_json(&self.directory.join("credentials.json"))?.unwrap_or(Document {
                version: 1,
                entries: BTreeMap::new(),
            });
        if doc.version != 1
            || doc
                .entries
                .iter()
                .any(|(id, value)| valid_id(id).is_err() || value.len() > 256 * 1024)
        {
            return Err("本地凭据文件内容无效，未覆盖原文件。".into());
        }
        Ok(doc)
    }
}
fn with_store<T>(service: &str, f: impl FnOnce(&Store) -> Result<T, String>) -> Result<T, String> {
    let stores = stores().lock().map_err(|_| "凭据存储锁不可用。")?;
    f(stores.get(service).ok_or("凭据存储尚未初始化。")?)
}
pub fn initialize(service: &str, directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|_| "无法创建凭据目录。")?;
    stores()
        .lock()
        .map_err(|_| "凭据存储锁不可用。")?
        .entry(service.into())
        .or_insert(Store {
            directory: directory.into(),
        });
    Ok(())
}
pub fn read(service: &str, id: &str) -> Result<Option<String>, String> {
    valid_id(id)?;
    with_store(service, |s| Ok(s.document()?.entries.get(id).cloned()))
}
pub fn save(service: &str, id: &str, value: &str) -> Result<(), String> {
    valid_id(id)?;
    if value.trim().is_empty() || value.len() > 8192 {
        return Err("凭据不能为空或超过 8192 字节。".into());
    }
    update(service, id, Some(value))
}
pub fn delete(service: &str, id: &str) -> Result<(), String> {
    valid_id(id)?;
    update(service, id, None)
}
fn update(service: &str, id: &str, value: Option<&str>) -> Result<(), String> {
    with_store(service, |s| {
        let _lock = lock(&s.directory)?;
        let mut doc = s.document()?;
        if let Some(value) = value {
            doc.entries.insert(id.into(), value.into());
        } else {
            doc.entries.remove(id);
        }
        atomic_json(&s.directory.join("credentials.json"), &doc)
    })
}
/// Original file values are imported only once. The source remains as a recovery copy.
pub fn pending_migration(service: &str) -> Result<Option<BTreeMap<String, String>>, String> {
    with_store(service, |s| {
        let marker = s.directory.join("credential-records-migrated.json");
        if let Some(version) = read_json::<u8>(&marker)? {
            if version != 1 {
                return Err("凭据迁移版本不受支持。".into());
            }
            return Ok(None);
        }
        let mut entries = s.document()?.entries;
        entries.remove(PASSWORD_KEY);
        Ok(Some(entries))
    })
}
pub fn finish_migration(service: &str) -> Result<(), String> {
    with_store(service, |s| {
        atomic_json(&s.directory.join("credential-records-migrated.json"), &1u8)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let service = uuid::Uuid::new_v4().to_string();
        initialize(&service, dir.path()).unwrap();
        (dir, service)
    }
    #[test]
    fn local_file_roundtrip_permissions_and_migration_marker() {
        let (dir, service) = fixture();
        save(&service, "connection", "sample-key").unwrap();
        save(&service, PASSWORD_KEY, "sample-password").unwrap();
        assert_eq!(
            read(&service, "connection").unwrap().as_deref(),
            Some("sample-key")
        );
        let pending = pending_migration(&service).unwrap().unwrap();
        assert_eq!(pending.len(), 1);
        assert!(!pending.contains_key(PASSWORD_KEY));
        finish_migration(&service).unwrap();
        assert!(pending_migration(&service).unwrap().is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.path().join("credentials.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn corrupted_and_future_files_are_preserved() {
        let (dir, service) = fixture();
        let path = dir.path().join("credentials.json");
        for contents in ["broken", r#"{"version":2,"entries":{}}"#] {
            fs::write(&path, contents).unwrap();
            assert!(save(&service, "id", "new").is_err());
            assert!(pending_migration(&service).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlink_is_rejected_without_overwriting_target() {
        let (dir, service) = fixture();
        let target = dir.path().join("other");
        fs::write(&target, "keep").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("credentials.json")).unwrap();
        assert!(save(&service, "id", "new").is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "keep");
    }
}
