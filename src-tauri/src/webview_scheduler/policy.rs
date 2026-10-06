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

    /// 窗口开着但失焦多久后降到休眠；None 表示失焦不降。
    /// 失焦就是用户去用别的程序了，正是 Low 的本意。渲染进程的 GC 堆平时空着一大半
    /// （实测首页 30 MB 里活对象只有 3.5 MB），Low 会把这些和脚本缓存一起还回去；页面几乎
    /// 没有大图，回来时重建的代价很小。聊天窗常开在一边当消息框，降得早一些；主窗和调试台
    /// 失焦常是切出去看一眼就回来，多等一会儿。
    pub(crate) fn idle_dormant_after(self) -> Option<Duration> {
        match self {
            Self::ChatPopout => Some(Duration::from_secs(60)),
            Self::Main | Self::DebugPopout => Some(Duration::from_secs(120)),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    SetVisible(bool),
    SetDormant(bool),
    ScheduleTick { after: Duration, generation: u64 },
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Entry {
    hidden: bool,
    focused: bool,
    dormant: bool,
    generation: u64,
}

impl Entry {
    pub(crate) fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub(crate) fn is_dormant(&self) -> bool {
        self.dormant
    }
}

pub(crate) fn step(
    role: WebviewRole,
    entry: &mut Entry,
    event: Event,
    _now: Instant,
) -> Vec<Action> {
    let mut actions = Vec::new();
    match event {
        Event::Hide => {
            entry.generation += 1;
            entry.focused = false;
            if !entry.hidden {
                entry.hidden = true;
                actions.push(Action::SetVisible(false));
            }
            if !entry.dormant {
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
    }
    actions
}

fn wake(entry: &mut Entry, actions: &mut Vec<Action>) {
    if entry.dormant {
        entry.dormant = false;
        actions.push(Action::SetDormant(false));
    }
    if entry.hidden {
        entry.hidden = false;
        actions.push(Action::SetVisible(true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(role: WebviewRole, entry: &mut Entry, event: Event) -> Vec<Action> {
        step(role, entry, event, Instant::now())
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
            vec![Action::SetVisible(true)]
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
        assert_eq!(after, Duration::from_secs(60));

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
            matches!(actions.as_slice(), [Action::ScheduleTick { after, .. }] if *after == Duration::from_secs(120))
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
}
