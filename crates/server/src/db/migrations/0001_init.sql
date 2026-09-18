-- 流转平台初始表结构
-- 时间统一使用本地时间的 'YYYY-MM-DD HH:MM:SS' 文本，便于按自然日分组统计。

-- ============ 组织与权限 ============

CREATE TABLE users (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    username             TEXT    NOT NULL UNIQUE,
    display_name         TEXT    NOT NULL,
    password_hash        TEXT    NOT NULL,
    phone                TEXT    NOT NULL DEFAULT '',
    dept                 TEXT    NOT NULL DEFAULT '',
    email                TEXT    NOT NULL DEFAULT '',
    status               INTEGER NOT NULL DEFAULT 1,   -- 1 启用 / 0 停用
    must_change_password INTEGER NOT NULL DEFAULT 0,
    failed_attempts      INTEGER NOT NULL DEFAULT 0,
    locked_until         TEXT,
    last_login_at        TEXT,
    created_at           TEXT    NOT NULL,
    updated_at           TEXT    NOT NULL
);

CREATE TABLE roles (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    code        TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    is_system   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL
);

CREATE TABLE permissions (
    code     TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    category TEXT NOT NULL,
    sort     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE role_permissions (
    role_id         INTEGER NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_code TEXT    NOT NULL REFERENCES permissions(code) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_code)
);

CREATE TABLE user_roles (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id INTEGER NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE INDEX idx_user_roles_role ON user_roles(role_id);

CREATE TABLE sessions (
    id         TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    ip         TEXT NOT NULL DEFAULT '',
    user_agent TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT
);

CREATE INDEX idx_sessions_user ON sessions(user_id);

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE audit_logs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    actor_id    INTEGER,
    actor_name  TEXT NOT NULL DEFAULT '',
    action      TEXT NOT NULL,
    target_type TEXT NOT NULL DEFAULT '',
    target_id   TEXT NOT NULL DEFAULT '',
    detail      TEXT NOT NULL DEFAULT '',
    ip          TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);

CREATE INDEX idx_audit_created ON audit_logs(created_at);

-- ============ 业务数据（外部导入） ============

CREATE TABLE field_defs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    code          TEXT    NOT NULL UNIQUE,
    label         TEXT    NOT NULL,
    field_type    TEXT    NOT NULL DEFAULT 'text',   -- text/number/date/select/bool
    required      INTEGER NOT NULL DEFAULT 0,
    is_unique_key INTEGER NOT NULL DEFAULT 0,        -- 作为导入去重主键
    options       TEXT    NOT NULL DEFAULT '[]',     -- select 类型的可选项 JSON 数组
    sort          INTEGER NOT NULL DEFAULT 0,
    show_in_list  INTEGER NOT NULL DEFAULT 1,
    searchable    INTEGER NOT NULL DEFAULT 1,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT    NOT NULL
);

CREATE TABLE import_templates (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    name               TEXT    NOT NULL,
    header_fingerprint TEXT    NOT NULL UNIQUE,
    mapping            TEXT    NOT NULL DEFAULT '{}',  -- { 系统字段code: 文件列序号 }
    dedupe_strategy    TEXT    NOT NULL DEFAULT 'upsert',
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL
);

CREATE TABLE import_batches (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    template_id  INTEGER REFERENCES import_templates(id) ON DELETE SET NULL,
    filename     TEXT    NOT NULL DEFAULT '',
    total_rows   INTEGER NOT NULL DEFAULT 0,
    inserted     INTEGER NOT NULL DEFAULT 0,
    updated      INTEGER NOT NULL DEFAULT 0,
    skipped      INTEGER NOT NULL DEFAULT 0,
    failed       INTEGER NOT NULL DEFAULT 0,
    errors       TEXT    NOT NULL DEFAULT '[]',
    operator_id  INTEGER,
    created_at   TEXT    NOT NULL
);

CREATE TABLE records (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    ext_key    TEXT    NOT NULL UNIQUE,
    batch_id   INTEGER,
    data       TEXT    NOT NULL DEFAULT '{}',
    created_by INTEGER,
    created_at TEXT    NOT NULL,
    updated_at TEXT    NOT NULL
);

CREATE INDEX idx_records_batch ON records(batch_id);

-- ============ 流转 ============

CREATE TABLE flow_defs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    code        TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    version     INTEGER NOT NULL DEFAULT 1,
    enabled     INTEGER NOT NULL DEFAULT 1,
    is_default  INTEGER NOT NULL DEFAULT 0,
    config      TEXT    NOT NULL DEFAULT '{}',
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE TABLE flow_instances (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    code             TEXT    NOT NULL UNIQUE,
    def_id           INTEGER NOT NULL REFERENCES flow_defs(id),
    record_id        INTEGER REFERENCES records(id) ON DELETE SET NULL,
    title            TEXT    NOT NULL,
    initiator_id     INTEGER NOT NULL REFERENCES users(id),
    status           INTEGER NOT NULL DEFAULT 1,   -- 1 进行中 / 2 已完成 / 3 已终止
    current_step_key TEXT    NOT NULL DEFAULT '',
    form             TEXT    NOT NULL DEFAULT '{}',
    created_at       TEXT    NOT NULL,
    updated_at       TEXT    NOT NULL,
    finished_at      TEXT
);

CREATE INDEX idx_flow_instances_status ON flow_instances(status);
CREATE INDEX idx_flow_instances_initiator ON flow_instances(initiator_id);

CREATE TABLE flow_tasks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    instance_id INTEGER NOT NULL REFERENCES flow_instances(id) ON DELETE CASCADE,
    step_key    TEXT    NOT NULL,
    step_name   TEXT    NOT NULL,
    step_type   TEXT    NOT NULL,
    assignee_id INTEGER NOT NULL REFERENCES users(id),
    assigned_by INTEGER,
    status      INTEGER NOT NULL DEFAULT 0,   -- 0 待办 / 1 已完成 / 2 已取消
    action      TEXT    NOT NULL DEFAULT '',
    comment     TEXT    NOT NULL DEFAULT '',
    result      TEXT    NOT NULL DEFAULT '{}',
    created_at  TEXT    NOT NULL,
    done_at     TEXT
);

CREATE INDEX idx_flow_tasks_assignee ON flow_tasks(assignee_id, status);
CREATE INDEX idx_flow_tasks_instance ON flow_tasks(instance_id);

CREATE TABLE flow_logs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    instance_id INTEGER NOT NULL REFERENCES flow_instances(id) ON DELETE CASCADE,
    actor_id    INTEGER,
    actor_name  TEXT    NOT NULL DEFAULT '',
    action      TEXT    NOT NULL,
    from_step   TEXT    NOT NULL DEFAULT '',
    to_step     TEXT    NOT NULL DEFAULT '',
    detail      TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL
);

CREATE INDEX idx_flow_logs_instance ON flow_logs(instance_id);

-- ============ 推送 / 通知渠道 ============

CREATE TABLE notifications (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category   TEXT    NOT NULL DEFAULT 'system',
    title      TEXT    NOT NULL,
    content    TEXT    NOT NULL DEFAULT '',
    ref_type   TEXT    NOT NULL DEFAULT '',
    ref_id     INTEGER,
    read_at    TEXT,
    created_at TEXT    NOT NULL
);

CREATE INDEX idx_notifications_user ON notifications(user_id, read_at);

CREATE TABLE notify_channels (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    channel    TEXT    NOT NULL,                  -- wecom/dingtalk/feishu/email
    name       TEXT    NOT NULL,
    config     TEXT    NOT NULL DEFAULT '{}',
    events     TEXT    NOT NULL DEFAULT '[]',
    enabled    INTEGER NOT NULL DEFAULT 0,
    created_at TEXT    NOT NULL
);

CREATE TABLE notify_logs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER REFERENCES notify_channels(id) ON DELETE CASCADE,
    event      TEXT    NOT NULL,
    payload    TEXT    NOT NULL DEFAULT '{}',
    success    INTEGER NOT NULL DEFAULT 0,
    error      TEXT    NOT NULL DEFAULT '',
    created_at TEXT    NOT NULL
);
