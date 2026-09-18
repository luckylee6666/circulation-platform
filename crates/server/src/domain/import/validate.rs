use chrono::NaiveDate;
use serde_json::{Value, json};

use crate::domain::field::{FieldDef, TYPE_BOOL, TYPE_DATE, TYPE_NUMBER, TYPE_SELECT};

/// 把单元格文本按字段类型转成入库值。返回 Err 时该项会被标记为「错误行」。
pub fn coerce(field: &FieldDef, raw: &str) -> Result<Value, String> {
    let raw = raw.trim();

    if raw.is_empty() {
        return if field.required {
            Err("必填项为空".to_string())
        } else {
            Ok(Value::Null)
        };
    }

    match field.field_type.as_str() {
        TYPE_NUMBER => parse_number(raw),
        TYPE_DATE => parse_date(raw),
        TYPE_BOOL => parse_bool(raw),
        TYPE_SELECT => {
            if field.options.iter().any(|option| option == raw) {
                Ok(Value::String(raw.to_string()))
            } else {
                Err(format!("「{raw}」不在可选范围内"))
            }
        }
        _ => Ok(Value::String(raw.to_string())),
    }
}

fn parse_number(raw: &str) -> Result<Value, String> {
    // 常见的中文数字格式：千分位逗号、全角字符、末尾的百分号
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, ',' | '，' | ' ' | '％' | '%'))
        .map(|c| match c {
            // 全角数字转半角：必须按 u32 相减，直接 as u8 会截断成低 8 位导致下溢
            '０'..='９' => char::from_u32('0' as u32 + (c as u32 - '０' as u32)).unwrap_or(c),
            '．' => '.',
            '－' => '-',
            other => other,
        })
        .collect();

    let value: f64 = cleaned
        .parse()
        .map_err(|_| format!("「{raw}」不是有效数字"))?;

    if value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        Ok(json!(value as i64))
    } else {
        Ok(json!(value))
    }
}

fn parse_date(raw: &str) -> Result<Value, String> {
    let text = raw.trim();

    // 纯数字的八位日期，例如 20240105
    if text.len() == 8
        && text.chars().all(|c| c.is_ascii_digit())
        && let Ok(date) = NaiveDate::parse_from_str(text, "%Y%m%d")
    {
        return Ok(Value::String(date.format("%Y-%m-%d").to_string()));
    }

    let date_only = [
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%Y.%m.%d",
        "%Y年%m月%d日",
        "%d/%m/%Y",
        "%m/%d/%Y",
    ];
    for format in date_only {
        if let Ok(date) = NaiveDate::parse_from_str(text, format) {
            return Ok(Value::String(date.format("%Y-%m-%d").to_string()));
        }
    }

    let with_time = [
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M",
    ];
    for format in with_time {
        if let Ok(value) = chrono::NaiveDateTime::parse_from_str(text, format) {
            return Ok(Value::String(value.format("%Y-%m-%d %H:%M:%S").to_string()));
        }
    }

    // 带时区的 ISO 字符串
    if let Ok(value) = chrono::DateTime::parse_from_rfc3339(text) {
        return Ok(Value::String(
            value.format("%Y-%m-%d %H:%M:%S").to_string(),
        ));
    }

    Err(format!("「{raw}」不是可识别的日期"))
}

fn parse_bool(raw: &str) -> Result<Value, String> {
    let normalized = raw.trim().to_lowercase();
    let value = match normalized.as_str() {
        "是" | "true" | "1" | "y" | "yes" | "真" | "有" | "√" => true,
        "否" | "false" | "0" | "n" | "no" | "假" | "无" | "×" => false,
        _ => return Err(format!("「{raw}」无法识别为是/否")),
    };
    Ok(json!(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::field::TYPE_TEXT;

    fn field(field_type: &str) -> FieldDef {
        FieldDef {
            id: 1,
            code: "f".into(),
            label: "字段".into(),
            field_type: field_type.into(),
            required: false,
            is_unique_key: false,
            options: vec![],
            sort: 0,
            show_in_list: true,
            searchable: true,
            enabled: true,
            created_at: String::new(),
        }
    }

    #[test]
    fn empty_required_field_is_an_error() {
        let mut def = field(TYPE_NUMBER);
        def.required = true;
        assert!(coerce(&def, "   ").is_err());
    }

    #[test]
    fn empty_optional_field_becomes_null() {
        assert_eq!(coerce(&field(TYPE_TEXT), "").unwrap(), Value::Null);
    }

    #[test]
    fn numbers_accept_chinese_formatting() {
        let def = field(TYPE_NUMBER);
        assert_eq!(coerce(&def, "1,234").unwrap(), json!(1234));
        assert_eq!(coerce(&def, "１２３").unwrap(), json!(123));
        assert_eq!(coerce(&def, "3.5").unwrap(), json!(3.5));
        assert!(coerce(&def, "abc").is_err());
    }

    #[test]
    fn dates_accept_common_formats() {
        let def = field(TYPE_DATE);
        for input in ["2026-01-05", "2026/1/5", "2026.01.05", "2026年1月5日", "20260105"] {
            assert_eq!(
                coerce(&def, input).unwrap(),
                json!("2026-01-05"),
                "输入 {input} 解析失败"
            );
        }
        assert_eq!(
            coerce(&def, "2026-01-05 08:30:00").unwrap(),
            json!("2026-01-05 08:30:00")
        );
        assert!(coerce(&def, "昨天").is_err());
    }

    #[test]
    fn booleans_accept_chinese_values() {
        let def = field(TYPE_BOOL);
        assert_eq!(coerce(&def, "是").unwrap(), json!(true));
        assert_eq!(coerce(&def, "否").unwrap(), json!(false));
        assert_eq!(coerce(&def, "1").unwrap(), json!(true));
        assert_eq!(coerce(&def, "NO").unwrap(), json!(false));
        assert!(coerce(&def, "也许").is_err());
    }

    #[test]
    fn select_rejects_values_outside_options() {
        let mut def = field(TYPE_SELECT);
        def.options = vec!["紧急".into(), "普通".into()];
        assert_eq!(coerce(&def, "紧急").unwrap(), json!("紧急"));
        assert!(coerce(&def, "特急").is_err());
    }
}
