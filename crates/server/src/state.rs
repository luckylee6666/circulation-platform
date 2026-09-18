use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::config::Config;
use crate::db::Pool;
use crate::domain::import::ImportStore;

/// 变更信号。
///
/// 刻意不带「这是谁的数据」——按人算推送目标需要在业务层把受影响的用户 id
/// 一路传出来，代码绕一大圈；而 30 人规模下让每个客户端收到信号后各拉各的，
/// 代价可以忽略。推送本身只是「尽快知道」，前端仍以拉取结果为准。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSignal {
    pub kind: String,
    pub at: String,
}

/// 记录每个用户最近一次发请求的时间。
///
/// 会话表只能说明「登录过」，判断不出「人还在不在」——一个关了浏览器的人，
/// 会话在过期前一直算有效。所以在线人数看的是最近几分钟有没有真的发过请求。
#[derive(Default)]
pub struct ActivityTracker {
    hits: Mutex<HashMap<i64, Instant>>,
}

impl ActivityTracker {
    pub fn touch(&self, user_id: i64) {
        if let Ok(mut hits) = self.hits.lock() {
            hits.insert(user_id, Instant::now());
        }
    }

    pub fn online_users(&self, window: Duration) -> usize {
        let Ok(hits) = self.hits.lock() else {
            return 0;
        };
        let now = Instant::now();
        hits.values()
            .filter(|at| now.duration_since(**at) < window)
            .count()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: Pool,
    pub imports: Arc<ImportStore>,
    pub activity: Arc<ActivityTracker>,
    events: broadcast::Sender<ChangeSignal>,
}

impl AppState {
    pub fn new(config: Config, pool: Pool) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            config: Arc::new(config),
            pool,
            imports: Arc::new(ImportStore::default()),
            activity: Arc::new(ActivityTracker::default()),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ChangeSignal> {
        self.events.subscribe()
    }

    /// 广播一次变更。没有订阅者时发送会失败，属正常情况。
    pub fn publish(&self, kind: &str) {
        let _ = self.events.send(ChangeSignal {
            kind: kind.to_string(),
            at: crate::util::now_str(),
        });
    }
}
