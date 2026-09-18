use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 桌面端本地设置，存成 `settings.json`。
/// 加了 `#[serde(default)]`，以后新增字段不会让旧配置文件解析失败。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub port: u16,
    pub autostart: bool,
    pub start_service_on_launch: bool,
    pub minimize_to_tray: bool,
    /// 留空表示使用默认的用户数据目录
    pub data_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: 8080,
            autostart: false,
            start_service_on_launch: true,
            minimize_to_tray: false,
            data_dir: None,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(path, body)
    }

    /// 实际使用的数据目录：用户自定义优先，否则用默认目录。
    pub fn resolved_data_dir(&self, default_dir: &Path) -> PathBuf {
        match self.data_dir.as_deref() {
            Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir.trim()),
            _ => default_dir.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let settings = Settings::default();
        assert_eq!(settings.port, 8080);
        assert!(settings.start_service_on_launch);
        assert!(!settings.autostart);
        assert!(settings.data_dir.is_none());
    }

    #[test]
    fn missing_file_falls_back_to_defaults() {
        let settings = Settings::load(Path::new("/definitely/not/here/settings.json"));
        assert_eq!(settings.port, 8080);
    }

    #[test]
    fn roundtrip_and_forward_compatible() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let settings = Settings {
            port: 9000,
            autostart: true,
            ..Settings::default()
        };
        settings.save(&path).unwrap();

        let loaded = Settings::load(&path);
        assert_eq!(loaded.port, 9000);
        assert!(loaded.autostart);

        // 旧版本写下的文件缺少新字段时，应该用默认值补齐而不是解析失败
        std::fs::write(&path, r#"{"port": 7777}"#).unwrap();
        let legacy = Settings::load(&path);
        assert_eq!(legacy.port, 7777);
        assert!(legacy.start_service_on_launch);
    }

    #[test]
    fn blank_data_dir_uses_default() {
        let default = Path::new("/tmp/default-data");
        let mut settings = Settings::default();
        assert_eq!(settings.resolved_data_dir(default), default);

        settings.data_dir = Some("   ".to_string());
        assert_eq!(settings.resolved_data_dir(default), default);

        settings.data_dir = Some("/custom/dir".to_string());
        assert_eq!(settings.resolved_data_dir(default), Path::new("/custom/dir"));
    }
}
