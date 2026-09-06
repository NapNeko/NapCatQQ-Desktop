// 定时自动重启：bot.json autoRestartSchedule 的解析 / 下次触发计算 + BotManager 调度环。
// legacy 是「手动启动时起一个周期 QTimer」；这里改成 30s 巡检 + 每 Bot 锚点，
// 配置改动无需重建定时器，cron 也能落到具体时刻。

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};
use croner::Cron;
use ncd_domain::bot_config::{AutoRestartMode, AutoRestartSchedule, BotConfigError};

use super::*;

/// cron 最小粒度是分钟，30s 巡检的触发抖动可接受。
pub const SCHEDULER_TICK: Duration = Duration::from_secs(30);

/// UI 预览默认列出的触发次数。
pub const PREVIEW_COUNT: usize = 3;

/// 标准 5 段（可选前置秒段），大小写 / 首尾空白不敏感。
pub fn parse_cron(expr: &str) -> Result<Cron, BotConfigError> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(BotConfigError::InvalidCron("表达式为空".to_string()));
    }
    Cron::from_str(trimmed).map_err(|err| BotConfigError::InvalidCron(err.to_string()))
}

/// 保存前校验：只有「启用 + Cron」需要解析；Interval 的 duration=0 不报错，
/// 调度时视为无计划（存量配置里可能有 0，不能因此拒绝加载）。
pub fn validate_schedule(schedule: &AutoRestartSchedule) -> Result<(), BotConfigError> {
    if schedule.enable && schedule.mode == AutoRestartMode::Cron {
        parse_cron(&schedule.cron)?;
    }
    Ok(())
}

/// since 之后的下一次触发；None = 无计划（未启用 / duration 0 / cron 无效或无未来匹配）。
pub fn next_fire_after<Tz: TimeZone>(
    schedule: &AutoRestartSchedule,
    since: &DateTime<Tz>,
) -> Option<DateTime<Tz>> {
    if !schedule.enable {
        return None;
    }
    match schedule.mode {
        AutoRestartMode::Interval => {
            let secs = i64::try_from(schedule.interval_seconds()?).ok()?;
            since
                .clone()
                .checked_add_signed(chrono::Duration::try_seconds(secs)?)
        }
        AutoRestartMode::Cron => parse_cron(&schedule.cron)
            .ok()?
            .find_next_occurrence(since, false)
            .ok(),
    }
}

/// 从 now 起的前 count 次触发，供 UI 校验 + 预览。
pub fn preview_cron<Tz: TimeZone>(
    expr: &str,
    now: &DateTime<Tz>,
    count: usize,
) -> Result<Vec<DateTime<Tz>>, BotConfigError> {
    let cron = parse_cron(expr)?;
    Ok(cron.iter_after(now.clone()).take(count).collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickDecision<Tz: TimeZone> {
    /// 到点，调用方去 restart
    Fire,
    /// 未到点；Some = 预计触发时刻
    Wait(Option<DateTime<Tz>>),
    /// 未启用 / 无有效计划
    Idle,
}

struct PlannerEntry<Tz: TimeZone> {
    /// 上次看到的计划；变了就重置锚点，避免刚改完配置立刻触发一次
    schedule: AutoRestartSchedule,
    /// 周期起点：最近一次进入 Running / 最近一次定时触发 / 计划变更时刻
    anchor: DateTime<Tz>,
}

/// 纯状态机，不持 BotManager；调度环负责喂事件与 tick。
pub struct RestartPlanner<Tz: TimeZone> {
    entries: HashMap<BotId, PlannerEntry<Tz>>,
}

impl<Tz: TimeZone> Default for RestartPlanner<Tz> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<Tz: TimeZone> RestartPlanner<Tz> {
    pub fn new() -> Self {
        Self::default()
    }

    /// 进入 Running（手动启动 / 重启 / reattach）：周期从现在重新算。
    /// 没有条目时不建，交给下一次 tick 用最新配置建。
    pub fn on_running(&mut self, bot_id: &BotId, now: DateTime<Tz>) {
        if let Some(entry) = self.entries.get_mut(bot_id) {
            entry.anchor = now;
        }
    }

    /// Stopped / Crashed / 删除：不再计时。
    pub fn on_inactive(&mut self, bot_id: &BotId) {
        self.entries.remove(bot_id);
    }

    pub fn on_tick(
        &mut self,
        bot_id: &BotId,
        schedule: &AutoRestartSchedule,
        now: DateTime<Tz>,
    ) -> TickDecision<Tz> {
        if !schedule.enable {
            self.entries.remove(bot_id);
            return TickDecision::Idle;
        }
        let entry = match self.entries.get_mut(bot_id) {
            Some(entry) if entry.schedule == *schedule => entry,
            _ => {
                let entry = self.entries.entry(bot_id.clone()).or_insert(PlannerEntry {
                    schedule: schedule.clone(),
                    anchor: now.clone(),
                });
                entry.schedule = schedule.clone();
                entry.anchor = now.clone();
                return TickDecision::Wait(next_fire_after(schedule, &entry.anchor));
            }
        };
        let Some(due) = next_fire_after(schedule, &entry.anchor) else {
            return TickDecision::Idle;
        };
        if now >= due {
            entry.anchor = now;
            TickDecision::Fire
        } else {
            TickDecision::Wait(Some(due))
        }
    }

    pub fn tracked(&self, bot_id: &BotId) -> bool {
        self.entries.contains_key(bot_id)
    }
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 长任务：订阅 BotStateChanged 维护锚点，每 SCHEDULER_TICK 巡检 Running 的 Bot。
    /// 到点走 RestartHandle::restart_bot（失败会发 bot_error），并 spawn 出去，
    /// 远端重启慢不能拖住巡检与事件消费。
    pub async fn run_auto_restart_scheduler(self: Arc<Self>) {
        let mut planner = RestartPlanner::<Local>::new();
        let mut state_sub = self
            .event_bus
            .subscribe(EventFilter::kind(DomainEventKind::BotStateChanged));
        let mut ticker = tokio::time::interval(SCHEDULER_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                ev = state_sub.next() => match ev {
                    Some(DomainEvent::BotStateChanged { snapshot, .. }) => match snapshot.state {
                        BotActorState::Running => planner.on_running(&snapshot.bot_id, Local::now()),
                        BotActorState::Stopped | BotActorState::Crashed => {
                            planner.on_inactive(&snapshot.bot_id)
                        }
                        _ => {}
                    },
                    Some(_) => {}
                    None => break,
                },
                _ = ticker.tick() => self.auto_restart_tick(&mut planner).await,
            }
        }
    }

    async fn auto_restart_tick(self: &Arc<Self>, planner: &mut RestartPlanner<Local>) {
        let running: Vec<BotId> = {
            let actors = self.actors.read().await;
            actors
                .values()
                .map(|handle| handle.snapshot())
                .filter(|snap| snap.state == BotActorState::Running)
                .map(|snap| snap.bot_id)
                .collect()
        };
        for bot_id in running {
            let Ok(Some(cfg)) = self.get_bot_config(&bot_id).await else {
                planner.on_inactive(&bot_id);
                continue;
            };
            let schedule = &cfg.bot.auto_restart_schedule;
            let was_tracked = planner.tracked(&bot_id);
            match planner.on_tick(&bot_id, schedule, Local::now()) {
                TickDecision::Fire => {
                    info!(
                        target: "ncd_runtime::bot_manager",
                        bot_id = %bot_id,
                        mode = ?schedule.mode,
                        "定时自动重启到点，触发重启"
                    );
                    let manager = Arc::clone(self);
                    tokio::spawn(async move {
                        RestartHandle::restart_bot(&*manager, &bot_id).await;
                    });
                }
                TickDecision::Wait(next) if !was_tracked => {
                    info!(
                        target: "ncd_runtime::bot_manager",
                        bot_id = %bot_id,
                        mode = ?schedule.mode,
                        next = next.map(|t| t.to_rfc3339()).as_deref().unwrap_or("-"),
                        "定时自动重启已排期"
                    );
                }
                TickDecision::Wait(_) => {}
                TickDecision::Idle => {
                    // 启用了 cron 却解析不出来：存量手改 bot.json 才会到这里，提示一次
                    if schedule.enable
                        && schedule.mode == AutoRestartMode::Cron
                        && !was_tracked
                    {
                        if let Err(err) = parse_cron(&schedule.cron) {
                            warn!(
                                target: "ncd_runtime::bot_manager",
                                bot_id = %bot_id,
                                %err,
                                "定时自动重启 cron 无效，本 Bot 不排期"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ncd_domain::bot_config::TimeUnit;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 6, h, m, 0).single().unwrap()
    }

    fn interval(hours: u32) -> AutoRestartSchedule {
        AutoRestartSchedule {
            enable: true,
            mode: AutoRestartMode::Interval,
            time_unit: TimeUnit::Hour,
            duration: hours,
            cron: String::new(),
        }
    }

    fn cron(expr: &str) -> AutoRestartSchedule {
        AutoRestartSchedule {
            enable: true,
            mode: AutoRestartMode::Cron,
            time_unit: TimeUnit::Hour,
            duration: 6,
            cron: expr.to_string(),
        }
    }

    fn bot() -> BotId {
        BotId::new("10001")
    }

    #[test]
    fn parse_cron_accepts_five_and_six_fields() {
        assert!(parse_cron("0 4 * * *").is_ok());
        assert!(parse_cron("30 0 4 * * *").is_ok());
        assert!(parse_cron("  */15 * * * *  ").is_ok());
        assert!(parse_cron("0 12 * * MON-FRI").is_ok());
    }

    #[test]
    fn parse_cron_rejects_garbage_and_empty() {
        assert!(matches!(parse_cron(""), Err(BotConfigError::InvalidCron(_))));
        assert!(matches!(
            parse_cron("   "),
            Err(BotConfigError::InvalidCron(_))
        ));
        assert!(matches!(
            parse_cron("every day at 4"),
            Err(BotConfigError::InvalidCron(_))
        ));
        assert!(matches!(
            parse_cron("0 25 * * *"),
            Err(BotConfigError::InvalidCron(_))
        ));
    }

    #[test]
    fn validate_schedule_only_checks_enabled_cron() {
        assert!(validate_schedule(&cron("0 4 * * *")).is_ok());
        assert!(validate_schedule(&cron("bad")).is_err());
        let disabled = AutoRestartSchedule {
            enable: false,
            ..cron("bad")
        };
        assert!(validate_schedule(&disabled).is_ok());
        assert!(validate_schedule(&interval(0)).is_ok());
    }

    #[test]
    fn next_fire_interval_adds_duration() {
        let next = next_fire_after(&interval(6), &at(1, 0)).unwrap();
        assert_eq!(next, at(7, 0));
        assert_eq!(next_fire_after(&interval(0), &at(1, 0)), None);
    }

    #[test]
    fn next_fire_cron_is_strictly_after_since() {
        let sched = cron("0 4 * * *");
        assert_eq!(next_fire_after(&sched, &at(1, 0)).unwrap(), at(4, 0));
        // 正好落在触发点上不算「下一次」
        let next = next_fire_after(&sched, &at(4, 0)).unwrap();
        assert_eq!(
            next,
            Utc.with_ymd_and_hms(2026, 9, 7, 4, 0, 0).single().unwrap()
        );
        assert_eq!(next_fire_after(&cron("bad"), &at(1, 0)), None);
    }

    #[test]
    fn preview_lists_upcoming_occurrences() {
        let list = preview_cron("0 */6 * * *", &at(1, 0), 3).unwrap();
        assert_eq!(list, vec![at(6, 0), at(12, 0), at(18, 0)]);
        assert!(preview_cron("bad", &at(1, 0), 3).is_err());
    }

    #[test]
    fn planner_first_tick_only_arms_and_fires_after_interval() {
        let mut planner = RestartPlanner::<Utc>::new();
        let sched = interval(1);
        assert_eq!(
            planner.on_tick(&bot(), &sched, at(1, 0)),
            TickDecision::Wait(Some(at(2, 0)))
        );
        assert_eq!(
            planner.on_tick(&bot(), &sched, at(1, 30)),
            TickDecision::Wait(Some(at(2, 0)))
        );
        assert_eq!(planner.on_tick(&bot(), &sched, at(2, 0)), TickDecision::Fire);
        // 触发后周期从触发点重算
        assert_eq!(
            planner.on_tick(&bot(), &sched, at(2, 30)),
            TickDecision::Wait(Some(at(3, 0)))
        );
    }

    #[test]
    fn planner_running_event_resets_anchor() {
        let mut planner = RestartPlanner::<Utc>::new();
        let sched = interval(1);
        planner.on_tick(&bot(), &sched, at(1, 0));
        // 1:50 手动重启完成 → 下一次是 2:50 而不是 2:00
        planner.on_running(&bot(), at(1, 50));
        assert_eq!(
            planner.on_tick(&bot(), &sched, at(2, 0)),
            TickDecision::Wait(Some(at(2, 50)))
        );
    }

    #[test]
    fn planner_schedule_change_rearms_instead_of_firing() {
        let mut planner = RestartPlanner::<Utc>::new();
        planner.on_tick(&bot(), &interval(6), at(1, 0));
        // 5 小时后改成 cron 04:00；04:00 已过，但不能因此立刻重启
        let sched = cron("0 4 * * *");
        assert!(matches!(
            planner.on_tick(&bot(), &sched, at(6, 0)),
            TickDecision::Wait(Some(_))
        ));
        assert_eq!(
            planner.on_tick(&bot(), &sched, at(23, 0)),
            TickDecision::Wait(Some(
                Utc.with_ymd_and_hms(2026, 9, 7, 4, 0, 0).single().unwrap()
            ))
        );
    }

    #[test]
    fn planner_cron_fires_once_per_occurrence() {
        let mut planner = RestartPlanner::<Utc>::new();
        let sched = cron("0 4 * * *");
        planner.on_tick(&bot(), &sched, at(1, 0));
        assert_eq!(
            planner.on_tick(&bot(), &sched, at(3, 59)),
            TickDecision::Wait(Some(at(4, 0)))
        );
        assert_eq!(planner.on_tick(&bot(), &sched, at(4, 0)), TickDecision::Fire);
        // 同一分钟内再巡检不重复触发
        assert!(matches!(
            planner.on_tick(&bot(), &sched, at(4, 0)),
            TickDecision::Wait(Some(_))
        ));
    }

    #[test]
    fn planner_disable_and_inactive_drop_entry() {
        let mut planner = RestartPlanner::<Utc>::new();
        planner.on_tick(&bot(), &interval(1), at(1, 0));
        assert!(planner.tracked(&bot()));
        planner.on_inactive(&bot());
        assert!(!planner.tracked(&bot()));

        planner.on_tick(&bot(), &interval(1), at(1, 0));
        let disabled = AutoRestartSchedule {
            enable: false,
            ..interval(1)
        };
        assert_eq!(
            planner.on_tick(&bot(), &disabled, at(1, 30)),
            TickDecision::Idle
        );
        assert!(!planner.tracked(&bot()));
    }

    #[test]
    fn planner_zero_duration_is_idle_but_tracked() {
        let mut planner = RestartPlanner::<Utc>::new();
        let sched = interval(0);
        assert_eq!(
            planner.on_tick(&bot(), &sched, at(1, 0)),
            TickDecision::Wait(None)
        );
        assert_eq!(planner.on_tick(&bot(), &sched, at(9, 0)), TickDecision::Idle);
    }
}
