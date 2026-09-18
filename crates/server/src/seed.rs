use rusqlite::Connection;

use crate::auth::password;
use crate::domain::user::UserInput;
use crate::error::AppResult;
use crate::util::now_str;

/// 权限点：(编码, 名称, 分组, 排序)
const PERMISSIONS: &[(&str, &str, &str, i64)] = &[
    ("user:manage", "用户管理", "组织权限", 10),
    ("role:manage", "角色权限", "组织权限", 20),
    ("field:manage", "字段定义", "数据配置", 30),
    ("data:view", "查看数据", "业务数据", 40),
    ("data:import", "导入数据", "业务数据", 50),
    ("data:edit", "编辑数据", "业务数据", 60),
    ("data:delete", "删除数据", "业务数据", 70),
    ("data:export", "导出数据", "业务数据", 80),
    ("flow:def:manage", "流程配置", "流转", 90),
    ("flow:create", "发起流转", "流转", 100),
    ("flow:dispatch", "分派任务", "流转", 110),
    ("flow:handle", "承办处理", "流转", 120),
    ("flow:confirm", "确认回复", "流转", 130),
    ("flow:terminate", "终止流转", "流转", 140),
    ("stats:view", "查看统计", "统计", 150),
    ("notify:channel:manage", "通知渠道", "系统", 160),
    ("settings:manage", "系统设置", "系统", 170),
    ("audit:view", "审计日志", "系统", 180),
];

/// 内置角色：(编码, 名称, 说明, 权限编码)
const ROLES: &[(&str, &str, &str, &[&str])] = &[
    (
        "admin",
        "系统管理员",
        "拥有全部权限，负责账号、角色与系统配置",
        &[
            "user:manage",
            "role:manage",
            "field:manage",
            "data:view",
            "data:import",
            "data:edit",
            "data:delete",
            "data:export",
            "flow:def:manage",
            "flow:create",
            "flow:dispatch",
            "flow:handle",
            "flow:confirm",
            "flow:terminate",
            "stats:view",
            "notify:channel:manage",
            "settings:manage",
            "audit:view",
        ],
    ),
    (
        "dispatcher",
        "调度员",
        "接收发起事项并分派给承办人，负责确认回复",
        &[
            "data:view",
            "data:import",
            "data:export",
            "flow:create",
            "flow:dispatch",
            "flow:confirm",
            "stats:view",
        ],
    ),
    (
        "handler",
        "承办人",
        "接收并处理分派到自己的任务",
        &["data:view", "flow:handle", "stats:view"],
    ),
    (
        "initiator",
        "发起人",
        "发起流转事项并确认办理结果",
        &["data:view", "flow:create", "flow:confirm", "stats:view"],
    ),
    (
        "viewer",
        "只读查看",
        "仅可查看数据与统计，不能进行任何操作",
        &["data:view", "stats:view"],
    ),
];

const DEFAULT_SETTINGS: &[(&str, &str)] = &[
    ("site_name", "流转平台"),
    ("allow_ip_whitelist", "off"),
    ("ip_whitelist", ""),
    ("notify_sound", "on"),
];

/// 幂等地写入初始数据：权限、内置角色、管理员账号。
pub fn apply(conn: &mut Connection) -> AppResult<()> {
    let tx = conn.transaction()?;

    seed_permissions(&tx)?;
    seed_roles(&tx)?;
    seed_settings(&tx)?;
    seed_default_flow(&tx)?;
    let created_admin = seed_admin(&tx)?;

    tx.commit()?;

    if created_admin {
        tracing::warn!("已创建默认管理员账号 admin，请首次登录后立即修改密码");
    }
    Ok(())
}

/// 首次运行写入一份能直接用的流程模板（1 发起 → 2 分派 → 承办 → 确认回复）。
fn seed_default_flow(conn: &Connection) -> AppResult<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM flow_defs", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }

    let config = crate::domain::flow::default_config();
    let now = now_str();
    conn.execute(
        "INSERT INTO flow_defs (code, name, description, version, enabled, is_default, config, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, 1, 1, ?4, ?5, ?5)",
        rusqlite::params![
            "default",
            "默认流转",
            "发起 → 分派 → 承办 → 确认回复；分派人手动选择承办人，确认不通过则退回分派",
            serde_json::to_string(&config).unwrap_or_else(|_| "{}".into()),
            now
        ],
    )?;

    tracing::info!("已写入默认流程模板");
    Ok(())
}

fn seed_permissions(conn: &Connection) -> AppResult<()> {
    for (code, name, category, sort) in PERMISSIONS {
        conn.execute(
            "INSERT INTO permissions (code, name, category, sort) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(code) DO UPDATE SET name = excluded.name, category = excluded.category, sort = excluded.sort",
            rusqlite::params![code, name, category, sort],
        )?;
    }
    Ok(())
}

fn seed_roles(conn: &Connection) -> AppResult<()> {
    let now = now_str();
    for (code, name, description, permissions) in ROLES {
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM roles WHERE code = ?1", [code], |row| row.get(0))
            .ok();

        let role_id = match exists {
            Some(id) => {
                conn.execute(
                    "UPDATE roles SET name = ?1, description = ?2 WHERE id = ?3",
                    rusqlite::params![name, description, id],
                )?;
                id
            }
            None => {
                conn.execute(
                    "INSERT INTO roles (code, name, description, is_system, created_at)
                     VALUES (?1, ?2, ?3, 1, ?4)",
                    rusqlite::params![code, name, description, now],
                )?;
                conn.last_insert_rowid()
            }
        };

        // 仅补齐缺失的权限，不覆盖管理员在界面上的自定义调整
        for permission in permissions.iter() {
            conn.execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_code) VALUES (?1, ?2)",
                rusqlite::params![role_id, permission],
            )?;
        }
    }
    Ok(())
}

fn seed_settings(conn: &Connection) -> AppResult<()> {
    let now = now_str();
    for (key, value) in DEFAULT_SETTINGS {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, value, now],
        )?;
    }
    Ok(())
}

fn seed_admin(conn: &Connection) -> AppResult<bool> {
    let user_count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    if user_count > 0 {
        return Ok(false);
    }

    let initial_password =
        std::env::var("CIRCULATION_ADMIN_PASSWORD").unwrap_or_else(|_| "admin123".to_string());
    let hash = password::hash_password(&initial_password)?;

    let input = UserInput {
        username: "admin".to_string(),
        display_name: "系统管理员".to_string(),
        password: initial_password,
        phone: String::new(),
        dept: "信息中心".to_string(),
        email: String::new(),
        role_ids: Vec::new(),
    };
    let admin_id = crate::domain::user::create(conn, &input, &hash)?;

    let role_id: i64 = conn.query_row("SELECT id FROM roles WHERE code = 'admin'", [], |row| row.get(0))?;
    conn.execute(
        "INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?1, ?2)",
        rusqlite::params![admin_id, role_id],
    )?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn seed_is_idempotent() {
        let mut conn = migrated();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();

        let roles: i64 = conn.query_row("SELECT COUNT(*) FROM roles", [], |r| r.get(0)).unwrap();
        let users: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap();
        let permissions: i64 = conn
            .query_row("SELECT COUNT(*) FROM permissions", [], |r| r.get(0))
            .unwrap();

        assert_eq!(roles, ROLES.len() as i64);
        assert_eq!(users, 1);
        assert_eq!(permissions, PERMISSIONS.len() as i64);
    }

    #[test]
    fn admin_has_every_permission() {
        let mut conn = migrated();
        apply(&mut conn).unwrap();

        let admin_id: i64 = conn.query_row("SELECT id FROM users WHERE username = 'admin'", [], |r| r.get(0)).unwrap();
        let granted = crate::domain::role::permissions_of_user(&conn, admin_id).unwrap();

        assert_eq!(granted.len(), PERMISSIONS.len());
        assert!(granted.contains("settings:manage"));
    }

    #[test]
    fn admin_password_comes_from_env_when_set() {
        let mut conn = migrated();
        // SAFETY: 测试内单线程设置环境变量
        unsafe { std::env::set_var("CIRCULATION_ADMIN_PASSWORD", "SuperSecret1") };
        apply(&mut conn).unwrap();
        unsafe { std::env::remove_var("CIRCULATION_ADMIN_PASSWORD") };

        let hash: String = conn
            .query_row("SELECT password_hash FROM users WHERE username = 'admin'", [], |r| r.get(0))
            .unwrap();
        assert!(password::verify_password("SuperSecret1", &hash));
    }
}
