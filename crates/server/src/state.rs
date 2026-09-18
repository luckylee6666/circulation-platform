use std::sync::Arc;

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

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: Pool,
    pub imports: Arc<ImportStore>,
    events: broadcast::Sender<ChangeSignal>,
}

impl AppState {
    pub fn new(config: Config, pool: Pool) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            config: Arc::new(config),
            pool,
            imports: Arc::new(ImportStore::default()),
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
