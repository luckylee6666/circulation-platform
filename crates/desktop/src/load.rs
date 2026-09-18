use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// 采样间隔。太密看不出趋势，太疏又会漏掉短时峰值。
const SAMPLE_INTERVAL: Duration = Duration::from_secs(30);

/// 保留 1 小时（120 × 30 秒）。
///
/// 这个窗口是给「长期挂着会不会涨内存」用的，所以宁可粗一点也要够长。
const HISTORY_CAPACITY: usize = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadPoint {
    /// 采样时刻，形如 14:32
    pub at: String,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentLoad {
    pub memory_bytes: u64,
    pub cpu_percent: f32,
}

/// 采集本进程的内存和 CPU 占用。
///
/// 服务是内嵌在桌面端进程里跑的，所以这里看的就是服务本身的负载。
pub struct LoadSampler {
    system: System,
    pid: Pid,
    history: VecDeque<LoadPoint>,
    last_sample: Option<Instant>,
}

impl Default for LoadSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadSampler {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            pid: Pid::from_u32(std::process::id()),
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
            last_sample: None,
        }
    }

    /// 刷新并返回当前占用；距上次采样满一个间隔就补一个历史点。
    ///
    /// CPU 占用是两次刷新之间的差值，所以每次调用都要刷新一次，
    /// 间隔由调用频率决定（控制台每 2 秒问一次，得到的就是 2 秒均值）。
    pub fn sample(&mut self) -> CurrentLoad {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );

        let Some(process) = self.system.process(self.pid) else {
            return CurrentLoad::default();
        };

        let load = CurrentLoad {
            memory_bytes: process.memory(),
            cpu_percent: process.cpu_usage(),
        };

        let due = self
            .last_sample
            .is_none_or(|last| last.elapsed() >= SAMPLE_INTERVAL);

        if due {
            if self.history.len() >= HISTORY_CAPACITY {
                self.history.pop_front();
            }
            self.history.push_back(LoadPoint {
                at: chrono::Local::now().format("%H:%M").to_string(),
                memory_bytes: load.memory_bytes,
            });
            self.last_sample = Some(Instant::now());
        }

        load
    }

    pub fn history(&self) -> Vec<LoadPoint> {
        self.history.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_reports_non_zero_memory() {
        let mut sampler = LoadSampler::new();
        let load = sampler.sample();

        // 当前进程必然占着内存，为 0 说明没读到
        assert!(load.memory_bytes > 0, "没读到进程内存占用");
    }

    #[test]
    fn first_sample_always_records_a_point() {
        let mut sampler = LoadSampler::new();
        assert!(sampler.history().is_empty());

        sampler.sample();

        let history = sampler.history();
        assert_eq!(history.len(), 1);
        assert!(history[0].memory_bytes > 0);
    }

    #[test]
    fn repeated_samples_within_interval_do_not_grow_history() {
        let mut sampler = LoadSampler::new();
        sampler.sample();
        sampler.sample();
        sampler.sample();

        // 间隔是 30 秒，连续采样只应该留下第一个点
        assert_eq!(sampler.history().len(), 1);
    }

    #[test]
    fn history_is_capped() {
        let mut sampler = LoadSampler::new();
        // 直接塞满，绕过时间间隔
        for _ in 0..HISTORY_CAPACITY + 10 {
            sampler.history.push_back(LoadPoint {
                at: "00:00".to_string(),
                memory_bytes: 1,
            });
            if sampler.history.len() > HISTORY_CAPACITY {
                sampler.history.pop_front();
            }
        }
        assert_eq!(sampler.history().len(), HISTORY_CAPACITY);
    }
}
