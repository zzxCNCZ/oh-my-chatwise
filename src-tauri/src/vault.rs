use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{
    aead, pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sealed {
    pub algorithm: String,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}
const AAD: &[u8] = b"chatwise-portable-config-v1";
fn key(password: &str, salt: &[u8]) -> Result<aead::LessSafeKey, String> {
    if password.len() < 8 || password.len() > 1024 {
        return Err("密码需要 8–1024 字节。".into());
    }
    let mut bytes = [0; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(600_000).unwrap(),
        salt,
        password.as_bytes(),
        &mut bytes,
    );
    aead::UnboundKey::new(&aead::AES_256_GCM, &bytes)
        .map(aead::LessSafeKey::new)
        .map_err(|_| "无法初始化配置加密。".into())
}
pub fn seal(bytes: &[u8], password: &str) -> Result<Sealed, String> {
    let rng = SystemRandom::new();
    let mut salt = [0; 16];
    let mut nonce = [0; 12];
    rng.fill(&mut salt)
        .and_then(|_| rng.fill(&mut nonce))
        .map_err(|_| "无法生成安全随机数。".to_string())?;
    let mut data = bytes.to_vec();
    key(password, &salt)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(AAD),
            &mut data,
        )
        .map_err(|_| "配置加密失败。".to_string())?;
    Ok(Sealed {
        algorithm: "AES-256-GCM/PBKDF2-SHA256-600000".into(),
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(data),
    })
}
pub fn open(sealed: &Sealed, password: &str) -> Result<Vec<u8>, String> {
    if sealed.algorithm != "AES-256-GCM/PBKDF2-SHA256-600000"
        || sealed.ciphertext.len() > 4 * 1024 * 1024
    {
        return Err("不支持的加密配置格式。".into());
    }
    let salt = STANDARD
        .decode(&sealed.salt)
        .map_err(|_| "无效的加密盐。")?;
    let nonce: [u8; 12] = STANDARD
        .decode(&sealed.nonce)
        .map_err(|_| "无效的加密 nonce。")?
        .try_into()
        .map_err(|_| "无效的加密 nonce。")?;
    if salt.len() != 16 {
        return Err("无效的加密盐。".into());
    }
    let mut data = STANDARD
        .decode(&sealed.ciphertext)
        .map_err(|_| "加密配置已损坏。")?;
    let plain = key(password, &salt)?
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(AAD),
            &mut data,
        )
        .map_err(|_| "密码错误或加密配置已损坏，未导入。".to_string())?;
    Ok(plain.to_vec())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_roundtrip_and_tamper_protection() {
        let sealed = seal("密钥与图床配置".as_bytes(), "test-password").unwrap();
        assert_eq!(
            open(&sealed, "test-password").unwrap(),
            "密钥与图床配置".as_bytes()
        );
        assert!(open(&sealed, "wrong-password").is_err());
        let mut tampered = sealed;
        tampered.salt = STANDARD.encode([1; 16]);
        assert!(open(&tampered, "test-password").is_err());
    }
}
