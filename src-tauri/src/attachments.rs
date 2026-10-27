use crate::models::{ChatMessage, ImageAttachment};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
pub const TEST_PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j9ZkAAAAASUVORK5CYII=";
pub fn validate_url(value: &str) -> Result<reqwest::Url, String> {
    let url =
        reqwest::Url::parse(value).map_err(|_| "请输入完整的 HTTP(S) 图片地址。".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("图片地址必须为不含用户名、密码或片段的 HTTP(S) URL。".into());
    }
    Ok(url)
}
pub fn media_type(bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok("image/png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Ok("image/jpeg");
    }
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok("image/webp");
    }
    Err("仅支持 PNG、JPEG 和 WebP 图片，文件内容必须与图片格式一致。".into())
}
fn image_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "无效的图片附件 ID。".to_string())?;
    Ok(directory.join("images").join(format!("{id}.bin")))
}
pub fn import(directory: &Path, name: &str, data_url: &str) -> Result<ImageAttachment, String> {
    if data_url.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 + 100 {
        return Err("单张图片不能超过 5 MB。".into());
    }
    let (prefix, data) = data_url.split_once(',').ok_or("无效的图片数据。")?;
    if !prefix.starts_with("data:image/") || !prefix.ends_with(";base64") {
        return Err("无效的图片 Base64 格式。".into());
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| "无法解码图片 Base64 数据。".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err("图片为空或超过 5 MB。".into());
    }
    let mime = media_type(&bytes)?;
    if prefix != format!("data:{mime};base64") {
        return Err("图片格式与实际内容不一致。".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let path = image_path(directory, &id)?;
    let parent = path.parent().ok_or("无法定位图片目录。")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建本地图片目录。".to_string())?;
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "无法创建图片文件。".to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "无法保存图片。".to_string())?;
    file.persist(path)
        .map_err(|_| "无法保存本地图片附件。".to_string())?;
    Ok(ImageAttachment {
        id,
        name: name.chars().take(255).collect(),
        media_type: mime.into(),
        size: bytes.len() as u64,
        source: "base64".into(),
        local: true,
        status: Some("ready".into()),
        ..Default::default()
    })
}
pub fn read(directory: &Path, id: &str) -> Result<Vec<u8>, String> {
    let path = image_path(directory, id)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| "找不到本地图片附件，请重新添加图片。".to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_IMAGE_BYTES as u64 {
        return Err("本地图片附件无效或超过大小限制。".into());
    }
    let bytes = fs::read(path).map_err(|_| "无法读取本地图片附件。".to_string())?;
    media_type(&bytes)?;
    Ok(bytes)
}
pub fn preview(directory: &Path, id: &str) -> Result<String, String> {
    let bytes = read(directory, id)?;
    Ok(format!(
        "data:{};base64,{}",
        media_type(&bytes)?,
        STANDARD.encode(bytes)
    ))
}
pub fn resolve(directory: &Path, messages: &mut [ChatMessage]) -> Result<(), String> {
    for message in messages {
        if message.images.len() > 5 || message.text_attachments.len() > 10 {
            return Err("每条消息最多支持 5 张图片和 10 个文本附件。".into());
        }
        let mut total = 0;
        if !message.images.is_empty() && message.role != "user" {
            return Err("图片附件只能出现在用户消息中。".into());
        }
        for image in &mut message.images {
            if matches!(
                image.status.as_deref(),
                Some("loading" | "uploading" | "error")
            ) {
                return Err("请等待图片处理完成，或移除失败的附件。".into());
            }
            match image.source.as_str() {
                "base64" => {
                    let bytes = read(directory, &image.id)?;
                    total += bytes.len();
                    image.media_type = media_type(&bytes)?.into();
                    image.data = Some(STANDARD.encode(bytes));
                }
                "url" => {
                    validate_url(image.url.as_deref().ok_or("图片 URL 为空。")?)?;
                }
                _ => return Err("不支持的图片来源。".into()),
            }
        }
        if total > 20 * 1024 * 1024 {
            return Err("每条消息的图片总大小不能超过 20 MB。".into());
        }
        if message
            .text_attachments
            .iter()
            .any(|file| file.context.len() > 512 * 1024)
        {
            return Err("文本附件内容超过大小限制。".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_images_separately_and_prevents_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let data = format!("data:image/png;base64,{TEST_PNG}");
        let mut image = import(dir.path(), "test.png", &data).unwrap();
        assert_eq!(preview(dir.path(), &image.id).unwrap(), data);
        assert!(read(dir.path(), "../../workspace.json").is_err());
        assert!(
            import(
                dir.path(),
                "fake.jpg",
                &data.replace("image/png", "image/jpeg")
            )
            .is_err()
        );
        assert!(import(dir.path(), "x.png", "data:image/png;base64,dGV4dA==").is_err());
        image.data = Some(TEST_PNG.into());
        assert!(!serde_json::to_string(&image).unwrap().contains(TEST_PNG));
    }
}
