use chrono::{DateTime, Local};
use ncd_domain::bot_config::{AutoRestartMode, AutoRestartSchedule};
use std::str::FromStr;

pub(super) fn next(
    schedule: &AutoRestartSchedule,
    now: DateTime<Local>,
) -> Option<DateTime<Local>> {
    if !schedule.enable {
        return None;
    }
    match schedule.mode {
        AutoRestartMode::Interval => now.checked_add_signed(chrono::Duration::try_seconds(
            i64::try_from(schedule.interval_seconds()?).ok()?,
        )?),
        AutoRestartMode::Cron => croner::Cron::from_str(schedule.cron.trim())
            .ok()?
            .find_next_occurrence(&now, false)
            .ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn disabled_and_invalid_schedules_do_not_restart() {
        let now = Local::now();
        assert!(next(&AutoRestartSchedule::default(), now).is_none());
        assert!(
            next(
                &AutoRestartSchedule {
                    enable: true,
                    mode: AutoRestartMode::Cron,
                    cron: "invalid".into(),
                    ..Default::default()
                },
                now
            )
            .is_none()
        );
    }

    #[test]
    fn interval_and_cron_are_strictly_after_the_anchor() {
        let now = Local
            .with_ymd_and_hms(2026, 10, 16, 12, 0, 0)
            .single()
            .unwrap();
        let schedule = AutoRestartSchedule {
            enable: true,
            duration: 1,
            ..Default::default()
        };
        assert_eq!(
            next(&schedule, now).unwrap(),
            now + chrono::Duration::hours(1)
        );
        let schedule = AutoRestartSchedule {
            mode: AutoRestartMode::Cron,
            cron: "0 12 * * *".into(),
            ..schedule
        };
        assert!(next(&schedule, now).unwrap() > now);
    }
}
