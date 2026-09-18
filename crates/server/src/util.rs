use chrono::{Duration, Local};

/// 本地时间字符串，数据库中所有时间的统一格式。
pub fn now_str() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn now_plus_hours(hours: i64) -> String {
    (Local::now() + Duration::hours(hours))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn now_plus_minutes(minutes: i64) -> String {
    (Local::now() + Duration::minutes(minutes))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 以今天为基准前后偏移的日期，用于补齐趋势图上的空白天。
pub fn date_offset_str(offset_days: i64) -> String {
    (Local::now() + Duration::days(offset_days))
        .format("%Y-%m-%d")
        .to_string()
}

pub fn date_before_days(days: i64) -> String {
    (Local::now() - Duration::days(days))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

/// 当前时刻往前推若干小时的时刻。
///
/// 统计里判断「超期」要在 Rust 侧算好边界再当字符串比较，
/// 直接用 SQL 的 `julianday('now')` 会踩 UTC 和本地时间的时差。
pub fn datetime_before_hours(hours: i64) -> String {
    (Local::now() - Duration::hours(hours))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn hex_decode(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(input.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

/// 定长比较，避免通过比较耗时推断出哈希内容。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 一组表头的指纹，用来识别「同一格式的文件」，从而复用上次的列映射。
pub fn headers_fingerprint(headers: &[String]) -> String {
    use sha2::{Digest, Sha256};

    let normalized: Vec<String> = headers.iter().map(|header| normalize_header(header)).collect();
    hex_encode(&Sha256::digest(normalized.join("|").as_bytes()))
}

/// 用于匹配表头：去掉空白和常见标点，统一成小写。
pub fn normalize_header(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '_' | '-' | ':' | '：' | '(' | ')' | '（' | '）'))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let raw = [0u8, 1, 15, 16, 127, 255];
        assert_eq!(hex_decode(&hex_encode(&raw)).unwrap(), raw);
    }

    #[test]
    fn hex_decode_rejects_invalid() {
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("zz").is_none());
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abcd", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abce"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
