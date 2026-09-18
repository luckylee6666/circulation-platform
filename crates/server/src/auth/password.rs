use argon2::Argon2;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::util::{constant_time_eq, hex_decode, hex_encode};

const SALT_LEN: usize = 16;
const HASH_LEN: usize = 32;
const PREFIX: &str = "argon2id";

fn hasher() -> Argon2<'static> {
    Argon2::default()
}

/// 生成 `argon2id$<salt_hex>$<hash_hex>` 格式的密码哈希。
pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = Uuid::new_v4().into_bytes();
    let mut out = [0u8; HASH_LEN];
    hasher()
        .hash_password_into(password.as_bytes(), &salt, &mut out)
        .map_err(|err| AppError::Internal(format!("密码哈希失败: {err}")))?;
    Ok(format!("{PREFIX}${}${}", hex_encode(&salt), hex_encode(&out)))
}

/// 校验密码，任何解析失败一律视为不匹配。
pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 3 || parts[0] != PREFIX {
        return false;
    }

    let (Some(salt), Some(expected)) = (hex_decode(parts[1]), hex_decode(parts[2])) else {
        return false;
    };
    if salt.len() != SALT_LEN || expected.len() != HASH_LEN {
        return false;
    }

    let mut out = [0u8; HASH_LEN];
    if hasher()
        .hash_password_into(password.as_bytes(), &salt, &mut out)
        .is_err()
    {
        return false;
    }
    constant_time_eq(&out, &expected)
}

/// 生成 256 位随机会话令牌。
pub fn generate_token() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

/// 会话令牌只存哈希，数据库泄露也无法直接冒用。
pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex_encode(&Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("Passw0rd!").unwrap();
        assert!(hash.starts_with("argon2id$"));
        assert!(verify_password("Passw0rd!", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn same_password_gets_different_salt() {
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert_ne!(a, b);
        assert!(verify_password("same", &a));
        assert!(verify_password("same", &b));
    }

    #[test]
    fn malformed_hash_is_rejected() {
        assert!(!verify_password("x", ""));
        assert!(!verify_password("x", "not-a-hash"));
        assert!(!verify_password("x", "argon2id$zz$zz"));
        assert!(!verify_password("x", "bcrypt$aa$bb"));
    }

    #[test]
    fn tokens_are_unique_and_hashable() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(hash_token(&a).len(), 64);
        assert_ne!(hash_token(&a), hash_token(&b));
    }
}
