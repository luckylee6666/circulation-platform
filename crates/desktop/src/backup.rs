use std::path::{Path, PathBuf};

use circulation_server::db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub file_name: String,
    pub path: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub backup: BackupFile,
    pub total: usize,
}

/// 用 SQLite 的 `VACUUM INTO` 生成一致性快照。
///
/// 直接复制文件在 WAL 模式下可能拿到「主库 + 半个 WAL」的损坏组合，
/// `VACUUM INTO` 由数据库自己导出，得到的永远是完整可用的库。
pub async fn create(pool: db::Pool, backup_dir: PathBuf) -> Result<BackupResult, String> {
    std::fs::create_dir_all(&backup_dir).map_err(|err| format!("无法创建备份目录：{err}"))?;

    let file_name = format!(
        "circulation-{}.db",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let target = backup_dir.join(&file_name);
    let target_for_task = target.clone();

    db::run(pool, move |conn| {
        // VACUUM INTO 要求目标文件不存在
        if target_for_task.exists() {
            std::fs::remove_file(&target_for_task)?;
        }
        conn.execute(
            "VACUUM INTO ?1",
            [target_for_task.to_string_lossy().to_string()],
        )?;
        Ok(())
    })
    .await
    .map_err(|err| format!("备份失败：{err}"))?;

    let size = std::fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
    let backups = list(&backup_dir)?;

    Ok(BackupResult {
        backup: BackupFile {
            file_name,
            path: target.display().to_string(),
            size,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        },
        total: backups.len(),
    })
}

/// 列出已有备份，最新的排在最前。
pub fn list(backup_dir: &Path) -> Result<Vec<BackupFile>, String> {
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(backup_dir).map_err(|err| format!("无法读取备份目录：{err}"))?;

    let mut backups: Vec<BackupFile> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("db") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let created_at = meta
                .modified()
                .ok()
                .map(|time| {
                    chrono::DateTime::<chrono::Local>::from(time)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();

            Some(BackupFile {
                file_name: path.file_name()?.to_string_lossy().to_string(),
                path: path.display().to_string(),
                size: meta.len(),
                created_at,
            })
        })
        .collect();

    backups.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    Ok(backups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_directory_yields_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let backups = list(&dir.path().join("not-created")).unwrap();
        assert!(backups.is_empty());
    }

    #[test]
    fn lists_only_db_files_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("circulation-20260101-090000.db"), b"a").unwrap();
        std::fs::write(dir.path().join("circulation-20260202-090000.db"), b"bb").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"ignore me").unwrap();

        let backups = list(dir.path()).unwrap();
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0].file_name, "circulation-20260202-090000.db");
        assert_eq!(backups[0].size, 2);
        assert_eq!(backups[1].file_name, "circulation-20260101-090000.db");
    }

    #[tokio::test]
    async fn vacuum_into_produces_a_readable_copy() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("data").join("circulation.db");
        let pool = db::init_pool(&db_path).unwrap();

        // 建一张表并写入数据，验证备份出来的是可用的库
        {
            let conn = pool.get().unwrap();
            conn.execute_batch("CREATE TABLE demo (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
                .unwrap();
            conn.execute("INSERT INTO demo (name) VALUES ('张三')", [])
                .unwrap();
        }

        let backup_dir = dir.path().join("backups");
        let result = create(pool, backup_dir.clone()).await.unwrap();

        assert!(result.backup.file_name.starts_with("circulation-"));
        assert!(result.backup.size > 0);
        assert_eq!(result.total, 1);

        // 用同一套连接配置打开备份，确认它是完整可读的库而不是半截文件
        let restored = db::init_pool(Path::new(&result.backup.path)).unwrap();
        let name: String = db::run(restored, |conn| {
            Ok(conn.query_row("SELECT name FROM demo WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })?)
        })
        .await
        .unwrap();
        assert_eq!(name, "张三");
    }
}
