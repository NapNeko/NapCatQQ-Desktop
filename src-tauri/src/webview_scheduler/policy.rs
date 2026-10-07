// 调度策略：窗口事件进来，算出要对 WebView 做的动作。纯逻辑，不碰 WebView2，便于单测。
//
// 级别从高到低：活跃 → 不可见（页面进 hidden，rAF 和动画停）→ 休眠（MemoryUsageTargetLevel
// = Low，WebView2 主动丢缓存）。藏起来的窗口直接降到休眠；开着但没焦点的窗口按角色决定
// 要不要过一段时间降休眠。挂起和回收不在这里：回收是各窗口自己的销毁逻辑（轻量模式），
// 挂起等验证完再加。

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebviewRole {
    Main,
    ChatPopout,
    DebugPopout,
    Other,
}

impl WebviewRole {
    pub(crate) fn from_label(label: &str) -> Self {
        match label {
            crate::lightweight::MAIN_WINDOW_LABEL => Self::Main,
            crate::chat_window::CHAT_WINDOW_LABEL => Self::ChatPopout,
            crate::commands::window::DEBUG_WINDOW_LABEL => Self::DebugPopout,
            _ => Self::Other,
        }
    }

    /// 给短暂切窗留余量，长期留在另一边的页面尽早归还缓存。
    pub(crate) fn idle_dormant_after(self) -> Option<Duration> {
        match self {
            Self::ChatPopout => Some(Duration::from_secs(15)),
            Self::Main => Some(Duration::from_secs(20)),
            Self::DebugPopout => Some(Duration::from_secs(120)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    Hide,
    Show,
    Focus(bool),
    /// 失焦计时到点，带安排时的代次；代次对不上说明中间有过别的事件，作废。
    Tick(u64),
    Retry(u64),
}

pub(crate) fn native_focus_event(focused: bool, visible: bool, minimized: bool) -> Option<Event> {
    if minimized {
        Some(Event::Hide)
    } else if visible {
        Some(Event::Focus(focused))
    } else {
        // 初始隐藏的窗口还要靠 rAF 完成首帧；隐藏后的迟到焦点也不能唤醒它。
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    SetVisible(bool),
    SetDormant(bool),
    ScheduleTick { after: Duration, generation: u64 },
    ScheduleRetry { generation: u64 },
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Entry {
    hidden: bool,
    focused: bool,
    dormant: bool,
    generation: u64,
    confirmed_hidden: Option<bool>,
    confirmed_dormant: Option<bool>,
    visibility_error: Option<String>,
    dormancy_error: Option<String>,
    retries: u8,
}

impl Entry {
    pub(crate) fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub(crate) fn is_dormant(&self) -> bool {
        self.dormant
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn confirmed_hidden(&self) -> Option<bool> {
        self.confirmed_hidden
    }

    pub(crate) fn confirmed_dormant(&self) -> Option<bool> {
        self.confirmed_dormant
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.visibility_error
            .as_deref()
            .or(self.dormancy_error.as_deref())
    }

    pub(crate) fn complete(
        &mut self,
        generation: u64,
        action: Action,
        result: Result<(), String>,
    ) -> Option<Action> {
        if generation != self.generation {
            return None;
        }
        let (confirmed, error, value) = match action {
            Action::SetVisible(visible) => (
                &mut self.confirmed_hidden,
                &mut self.visibility_error,
                !visible,
            ),
            Action::SetDormant(dormant) => (
                &mut self.confirmed_dormant,
                &mut self.dormancy_error,
                dormant,
            ),
            _ => return None,
        };
        match result {
            Ok(()) => {
                *confirmed = Some(value);
                *error = None;
                None
            }
            Err(reason) => {
                *error = Some(reason);
                if self.retries >= 2 {
                    return None;
                }
                self.retries += 1;
                Some(Action::ScheduleRetry { generation })
            }
        }
    }
}

pub(crate) fn step(
    role: WebviewRole,
    entry: &mut Entry,
    event: Event,
    _now: Instant,
) -> Vec<Action> {
    let mut actions = Vec::new();
    if !matches!(event, Event::Retry(_) | Event::Tick(_)) {
        entry.retries = 0;
    }
    match event {
        Event::Hide => {
            entry.generation += 1;
            entry.focused = false;
            if !entry.hidden
                || entry.confirmed_hidden != Some(true)
                || entry.visibility_error.is_some()
            {
                entry.hidden = true;
                actions.push(Action::SetVisible(false));
            }
            if !entry.dormant
                || entry.confirmed_dormant != Some(true)
                || entry.dormancy_error.is_some()
            {
                entry.dormant = true;
                actions.push(Action::SetDormant(true));
            }
        }
        Event::Show => {
            entry.generation += 1;
            wake(entry, &mut actions);
            // 显示时总是放一次可见：调度器之外有人藏过 WebView 时也能拉回来，重复设可见没有代价
            if !actions.contains(&Action::SetVisible(true)) {
                actions.push(Action::SetVisible(true));
            }
        }
        Event::Focus(true) => {
            entry.generation += 1;
            entry.focused = true;
            // 某条显示路径没经过调度器，窗口已经在前台了 WebView 还是藏着的：在这里补上
            wake(entry, &mut actions);
        }
        Event::Focus(false) => {
            entry.focused = false;
            if !entry.hidden
                && !entry.dormant
                && let Some(after) = role.idle_dormant_after()
            {
                entry.generation += 1;
                actions.push(Action::ScheduleTick {
                    after,
                    generation: entry.generation,
                });
            }
        }
        Event::Tick(generation) => {
            if generation == entry.generation && !entry.focused && !entry.hidden && !entry.dormant {
                entry.dormant = true;
                actions.push(Action::SetDormant(true));
            }
        }
        Event::Retry(generation) => {
            if generation == entry.generation {
                if entry.visibility_error.is_some() {
                    actions.push(Action::SetVisible(!entry.hidden));
                }
                if entry.dormancy_error.is_some() {
                    actions.push(Action::SetDormant(entry.dormant));
                }
            }
        }
    }
    actions
}

fn wake(entry: &mut Entry, actions: &mut Vec<Action>) {
    if entry.dormant || entry.confirmed_dormant != Some(false) || entry.dormancy_error.is_some() {
        entry.dormant = false;
        actions.push(Action::SetDormant(false));
    }
    if entry.hidden || entry.confirmed_hidden != Some(false) || entry.visibility_error.is_some() {
        entry.hidden = false;
        actions.push(Action::SetVisible(true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(role: WebviewRole, entry: &mut Entry, event: Event) -> Vec<Action> {
        let actions = step(role, entry, event, Instant::now());
        for action in &actions {
            entry.complete(entry.generation(), *action, Ok(()));
        }
        actions
    }

    #[test]
    fn hide_goes_straight_to_dormant_and_show_wakes_up_once() {
        let mut e = Entry::default();
        assert_eq!(
            run(WebviewRole::Main, &mut e, Event::Hide),
            vec![Action::SetVisible(false), Action::SetDormant(true)]
        );
        // 重复藏不重复下发
        assert!(run(WebviewRole::Main, &mut e, Event::Hide).is_empty());
        assert_eq!(
            run(WebviewRole::Main, &mut e, Event::Show),
            vec![Action::SetDormant(false), Action::SetVisible(true)]
        );
        assert!(!e.is_hidden() && !e.is_dormant());
    }

    #[test]
    fn show_always_restores_visibility_even_when_state_thinks_visible() {
        let mut e = Entry::default();
        assert_eq!(
            run(WebviewRole::Other, &mut e, Event::Show),
            vec![Action::SetDormant(false), Action::SetVisible(true)]
        );
    }

    #[test]
    fn focus_recovers_a_window_shown_behind_the_schedulers_back() {
        let mut e = Entry::default();
        run(WebviewRole::Main, &mut e, Event::Hide);
        assert_eq!(
            run(WebviewRole::Main, &mut e, Event::Focus(true)),
            vec![Action::SetDormant(false), Action::SetVisible(true)]
        );
        // 正常可见时拿焦点什么都不做
        assert!(run(WebviewRole::Main, &mut e, Event::Focus(true)).is_empty());
    }

    #[test]
    fn chat_popout_goes_dormant_after_idle_but_focus_in_between_cancels() {
        let mut e = Entry::default();
        let actions = run(WebviewRole::ChatPopout, &mut e, Event::Focus(false));
        let Some(Action::ScheduleTick { after, generation }) = actions.first().copied() else {
            panic!("失焦应安排计时: {actions:?}");
        };
        assert_eq!(after, Duration::from_secs(15));

        run(WebviewRole::ChatPopout, &mut e, Event::Focus(true));
        assert!(
            run(WebviewRole::ChatPopout, &mut e, Event::Tick(generation)).is_empty(),
            "中途拿过焦点，旧计时作废"
        );

        let actions = run(WebviewRole::ChatPopout, &mut e, Event::Focus(false));
        let Some(Action::ScheduleTick { generation, .. }) = actions.first().copied() else {
            panic!("再次失焦应重新计时");
        };
        assert_eq!(
            run(WebviewRole::ChatPopout, &mut e, Event::Tick(generation)),
            vec![Action::SetDormant(true)]
        );
        assert_eq!(
            run(WebviewRole::ChatPopout, &mut e, Event::Focus(true)),
            vec![Action::SetDormant(false)]
        );
    }

    #[test]
    fn main_window_waits_longer_than_chat_before_going_dormant() {
        let mut e = Entry::default();
        let actions = run(WebviewRole::Main, &mut e, Event::Focus(false));
        assert!(
            matches!(actions.as_slice(), [Action::ScheduleTick { after, .. }] if *after == Duration::from_secs(20))
        );
        // 没登记角色的窗口失焦不另外计时
        assert!(
            run(
                WebviewRole::Other,
                &mut Entry::default(),
                Event::Focus(false)
            )
            .is_empty()
        );
    }

    #[test]
    fn stale_tick_after_hide_is_ignored() {
        let mut e = Entry::default();
        let actions = run(WebviewRole::ChatPopout, &mut e, Event::Focus(false));
        let Some(Action::ScheduleTick { generation, .. }) = actions.first().copied() else {
            panic!("失焦应安排计时");
        };
        run(WebviewRole::ChatPopout, &mut e, Event::Hide);
        assert!(run(WebviewRole::ChatPopout, &mut e, Event::Tick(generation)).is_empty());
    }

    #[test]
    fn roles_follow_window_labels() {
        assert_eq!(WebviewRole::from_label("main"), WebviewRole::Main);
        assert_eq!(
            WebviewRole::from_label(crate::chat_window::CHAT_WINDOW_LABEL),
            WebviewRole::ChatPopout
        );
        assert_eq!(
            WebviewRole::from_label("debug-console"),
            WebviewRole::DebugPopout
        );
        assert_eq!(
            WebviewRole::from_label("something-else"),
            WebviewRole::Other
        );
    }

    #[test]
    fn hidden_native_focus_does_not_wake_or_hide_a_booting_page() {
        let mut entry = Entry::default();
        run(WebviewRole::Main, &mut entry, Event::Hide);
        let generation = entry.generation();
        assert_eq!(native_focus_event(true, false, false), None);
        assert_eq!(native_focus_event(false, false, false), None);
        assert!(entry.is_hidden() && entry.is_dormant());
        assert_eq!(entry.generation(), generation);
    }

    #[test]
    fn minimizing_releases_the_page_and_visible_focus_restores_it() {
        let mut entry = Entry::default();
        let event = native_focus_event(false, true, true).unwrap();
        assert_eq!(
            run(WebviewRole::ChatPopout, &mut entry, event),
            vec![Action::SetVisible(false), Action::SetDormant(true)]
        );
        let restored = native_focus_event(true, true, false).unwrap();
        assert_eq!(
            run(WebviewRole::ChatPopout, &mut entry, restored),
            vec![Action::SetDormant(false), Action::SetVisible(true)]
        );
    }

    #[test]
    fn failed_low_is_unconfirmed_and_retries_are_bounded() {
        let mut e = Entry::default();
        step(WebviewRole::Main, &mut e, Event::Hide, Instant::now());
        let generation = e.generation();
        for attempt in 0..3 {
            let retry = e.complete(
                generation,
                Action::SetDormant(true),
                Err("COM failed".into()),
            );
            assert_eq!(retry.is_some(), attempt < 2);
        }
        assert!(e.is_dormant());
        assert_eq!(e.confirmed_dormant(), None);
        assert_eq!(e.last_error(), Some("COM failed"));
        assert_eq!(
            run(WebviewRole::Main, &mut e, Event::Retry(generation)),
            vec![Action::SetDormant(true)]
        );
        e.complete(generation, Action::SetDormant(true), Ok(()));
        assert_eq!(e.confirmed_dormant(), Some(true));
        assert_eq!(e.last_error(), None);
    }

    #[test]
    fn stale_completion_and_retry_cannot_overwrite_a_restored_window() {
        let mut e = Entry::default();
        run(WebviewRole::Main, &mut e, Event::Hide);
        let old = e.generation();
        run(WebviewRole::Main, &mut e, Event::Show);
        e.complete(e.generation(), Action::SetDormant(false), Ok(()));
        assert!(
            e.complete(old, Action::SetDormant(true), Err("late".into()))
                .is_none()
        );
        assert_eq!(e.confirmed_dormant(), Some(false));
        assert_eq!(e.last_error(), None);
        assert!(run(WebviewRole::Main, &mut e, Event::Retry(old)).is_empty());
    }

    #[test]
    fn a_new_generation_reissues_unconfirmed_low_and_normal() {
        let mut e = Entry::default();
        let hide = step(WebviewRole::Main, &mut e, Event::Hide, Instant::now());
        assert_eq!(
            step(WebviewRole::Main, &mut e, Event::Hide, Instant::now()),
            hide
        );
        let show = step(WebviewRole::Main, &mut e, Event::Show, Instant::now());
        assert_eq!(
            step(
                WebviewRole::Main,
                &mut e,
                Event::Focus(true),
                Instant::now()
            ),
            show
        );
    }
}
