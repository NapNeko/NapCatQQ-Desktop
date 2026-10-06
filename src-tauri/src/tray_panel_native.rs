// 主托盘右键面板（原生 Direct2D，宽 260 DIP）。
#![warn(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use tauri::Manager;

use crate::avatar_cache::{AvatarKey, AvatarPixels};
use crate::native_panel::gfx::{Canvas, Image, Rect, TextStyle};
use crate::native_panel::icons::Icon;
use crate::native_panel::parts::{
    ACTION_H, ActionRow, BUTTON_RADIUS, HEADER_H, HEADER_ICON, Header, SEPARATOR_H, Text, centered,
    draw, initial, separator, text,
};
use crate::native_panel::theme::{Rgba, Theme, UiPreferences, set_ui_preferences};
use crate::native_panel::window::{NativePanel, PanelContent};
use crate::tray_avatars::{self, AvatarSlots};

const WIDTH: f32 = 260.0;
const PAGE_SIZE: usize = 2;

#[derive(Clone)]
pub struct BotCard {
    pub bot_id: String,
    pub state: String,
    pub name: String,
    pub qq: String,
    pub is_snowluma: bool,
    /// NapCat 且 webui 端点已就绪：(端口, token)
    pub webui: Option<(u16, String)>,
}

impl BotCard {
    fn state_str(s: &ncd_domain::bot_actor::BotActorState) -> &'static str {
        use ncd_domain::bot_actor::BotActorState as S;
        match s {
            S::Stopped => "stopped",
            S::Starting => "starting",
            S::Running => "running",
            S::Stopping => "stopping",
            S::Crashed => "crashed",
            S::Repairing => "starting",
        }
    }

    fn running(&self) -> bool {
        self.state == "running"
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub cards: Vec<BotCard>,
}

impl Snapshot {
    fn total_pages(&self) -> usize {
        self.cards.len().div_ceil(PAGE_SIZE).max(1)
    }
    fn running_count(&self) -> usize {
        self.cards.iter().filter(|c| c.running()).count()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    StartStop(usize),
    OpenWebui(usize),
    OpenNovnc(usize),
    PagePrev,
    PageNext,
    ShowMain,
    Lightweight,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CardButton {
    Novnc,
    Webui,
    StartStop,
}

enum RowKind {
    Header {
        header: Header,
        running: bool,
    },
    Error {
        text: Option<Text>,
    },
    Separator,
    Pager {
        label: Option<Text>,
        page: Option<Text>,
        prev: Rect,
        next: Rect,
        prev_on: bool,
        next_on: bool,
    },
    Card {
        index: usize,
        avatar: Rect,
        initial: Option<Text>,
        name: Option<Text>,
        qq: Option<Text>,
        dot: (f32, f32),
        buttons: Vec<(CardButton, Rect)>,
    },
    Action {
        row: ActionRow,
        action: Action,
    },
}

struct Row {
    rect: Rect,
    kind: RowKind,
}

struct Layout {
    rows: Vec<Row>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hot {
    Row(usize),
    Button(usize, CardButton),
    Prev,
    Next,
}

/// 纯视图：数据 + 布局 + 绘制 + 命中，不碰 AppHandle，测试直接构造。
pub(crate) struct TrayView {
    snapshot: Snapshot,
    page: usize,
    layout: Option<Layout>,
    hovered: Option<Hot>,
    pending: HashSet<String>,
    error: Option<String>,
    spinner: f32,
    pulse_ms: u32,
    logo: Option<Rc<Image>>,
    avatars: AvatarSlots,
}

const PALETTE: [(Rgba, Rgba); 6] = [
    (Rgba::rgb(0xf9, 0xa8, 0xd4), Rgba::rgb(0xfb, 0x71, 0x85)),
    (Rgba::rgb(0xfc, 0xd3, 0x4d), Rgba::rgb(0xfb, 0x92, 0x3c)),
    (Rgba::rgb(0x6e, 0xe7, 0xb7), Rgba::rgb(0x2d, 0xd4, 0xbf)),
    (Rgba::rgb(0x7d, 0xd3, 0xfc), Rgba::rgb(0x81, 0x8c, 0xf8)),
    (Rgba::rgb(0xc4, 0xb5, 0xfd), Rgba::rgb(0xe8, 0x79, 0xf9)),
    (Rgba::rgb(0xfd, 0xa4, 0xaf), Rgba::rgb(0xf8, 0x71, 0x71)),
];

/// 和 Bot 卡片（botCardParts.tsx）的 pickAvatarPalette 同一个哈希：按 UTF-16 码元 h = h * 31 + c（i32 溢出回绕）。
fn avatar_palette(seed: &str) -> (Rgba, Rgba) {
    let h = seed
        .encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)));
    let index = (h.unsigned_abs() % PALETTE.len() as u32) as usize;
    PALETTE.get(index).copied().unwrap_or(PALETTE[0])
}

impl TrayView {
    pub(crate) fn new(logo: Option<Rc<Image>>) -> Self {
        Self {
            snapshot: Snapshot::default(),
            page: 1,
            layout: None,
            hovered: None,
            pending: HashSet::new(),
            error: None,
            spinner: 0.0,
            pulse_ms: 0,
            logo,
            avatars: AvatarSlots::default(),
        }
    }

    fn current_page(&self) -> usize {
        self.page.clamp(1, self.snapshot.total_pages())
    }

    fn visible(&self) -> std::ops::Range<usize> {
        let start = (self.current_page() - 1) * PAGE_SIZE;
        start.min(self.snapshot.cards.len())..(start + PAGE_SIZE).min(self.snapshot.cards.len())
    }

    fn transitioning(&self, card: &BotCard) -> bool {
        self.pending.contains(&card.bot_id)
            || matches!(card.state.as_str(), "starting" | "stopping")
    }

    fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
        self.page = self.current_page();
        self.layout = None;
        self.error = None;
        self.hovered = None;
    }

    /// 当前页要显示的头像；顺带把不在快照里的槽位清掉。
    fn claim_avatars(&mut self) -> Vec<AvatarKey> {
        let all: HashSet<AvatarKey> = self
            .snapshot
            .cards
            .iter()
            .filter_map(|c| AvatarKey::user(&c.qq))
            .collect();
        self.avatars.retain(&all);
        let wanted: Vec<AvatarKey> = self
            .snapshot
            .cards
            .get(self.visible())
            .unwrap_or_default()
            .iter()
            .filter_map(|c| AvatarKey::user(&c.qq))
            .collect();
        self.avatars.claim(wanted)
    }

    fn status_text(&self) -> String {
        let total = self.snapshot.cards.len();
        let running = self.snapshot.running_count();
        if total == 0 {
            "后台待命".into()
        } else if running > 0 {
            format!("{running}/{total} 个 Bot 运行中")
        } else {
            "全部已停止".into()
        }
    }

    /// 按 WebView 的盒模型从上往下排：头部 39.5，分隔线 my-0.5 = 5 / my-1 = 9，卡片 42，操作行 26。
    fn layout(&mut self, width: f32) -> f32 {
        let mut rows = Vec::new();
        rows.push(Row {
            rect: Rect::new(0.0, 0.0, width, HEADER_H),
            kind: RowKind::Header {
                header: Header::new(width, "NapCatQQ-Desktop", &self.status_text()),
                running: self.snapshot.running_count() > 0,
            },
        });
        let mut y = HEADER_H;

        if let Some(err) = self.error.as_deref().filter(|e| !e.is_empty()) {
            // mx-2 mb-1 px-2 py-1 text-[11px] leading-snug
            let line = 11.0 * 1.375;
            let rect = Rect::new(8.0, y, width - 16.0, line + 8.0);
            rows.push(Row {
                rect,
                kind: RowKind::Error {
                    text: text(
                        err,
                        TextStyle::sans(11.0, 400, line),
                        rect.w - 16.0,
                        16.0,
                        y + 4.0,
                    ),
                },
            });
            y += rect.h + 4.0;
        }

        if !self.snapshot.cards.is_empty() {
            rows.push(Row {
                rect: Rect::new(8.0, y + 4.0, width - 16.0, 1.0),
                kind: RowKind::Separator,
            });
            y += 9.0;

            let pages = self.snapshot.total_pages();
            if pages > 1 {
                // px-2.5 pb-1，justify-between：右侧 [‹ 20] 6 [页码] 6 [› 20] 贴右，左侧标签吃剩下的
                let current = self.current_page();
                let next = Rect::new(width - 10.0 - 20.0, y, 20.0, 20.0);
                let page = text(
                    &format!("{current} / {pages}"),
                    TextStyle::mono(11.0, 400, 11.0),
                    80.0,
                    0.0,
                    y + 4.5,
                )
                .map(|mut t| {
                    t.x = next.x - 6.0 - t.tb.width;
                    t
                });
                let page_x = page.as_ref().map_or(next.x - 6.0, |t| t.x);
                let prev = Rect::new(page_x - 6.0 - 20.0, y, 20.0, 20.0);
                rows.push(Row {
                    rect: Rect::new(0.0, y, width, 20.0),
                    kind: RowKind::Pager {
                        label: text(
                            &format!("机器人列表 ({})", self.snapshot.cards.len()),
                            TextStyle::sans(11.0, 400, 16.5),
                            prev.x - 10.0,
                            10.0,
                            y + 1.75,
                        ),
                        page,
                        prev,
                        next,
                        prev_on: current > 1,
                        next_on: current < pages,
                    },
                });
                y += 24.0;
            }

            // flex-col gap-1 px-2 py-0.5
            y += 2.0;
            let range = self.visible();
            let count = range.len();
            for (n, index) in range.enumerate() {
                let Some(card) = self.snapshot.cards.get(index) else {
                    continue;
                };
                let rect = Rect::new(8.0, y, width - 16.0, 42.0);
                rows.push(Row {
                    rect,
                    kind: self.card_kind(index, card, rect),
                });
                y += 42.0;
                if n + 1 < count {
                    y += 4.0;
                }
            }
            y += 2.0;
        }

        rows.push(Row {
            rect: Rect::new(8.0, y + 2.0, width - 16.0, 1.0),
            kind: RowKind::Separator,
        });
        y += SEPARATOR_H;
        // 操作组 px-1 py-0.5，最后一组 pt-0.5 pb-1
        y += 2.0;
        for (icon, label, action) in [
            (Icon::PanelsTopLeft, "显示主窗口", Action::ShowMain),
            (Icon::BatteryCharging, "释放界面内存", Action::Lightweight),
        ] {
            rows.push(action_row(width, y, icon, label, action));
            y += ACTION_H;
        }
        y += 2.0;
        rows.push(Row {
            rect: Rect::new(8.0, y + 2.0, width - 16.0, 1.0),
            kind: RowKind::Separator,
        });
        y += SEPARATOR_H;
        y += 2.0;
        rows.push(action_row(width, y, Icon::LogOut, "退出", Action::Quit));
        y += ACTION_H + 4.0;

        self.layout = Some(Layout { rows });
        y
    }

    /// 卡片 px-2 py-1.5：头像 30 在左，右侧组（状态点、按钮 24）贴右，文字列在中间吃剩下的宽。
    fn card_kind(&self, index: usize, card: &BotCard, rect: Rect) -> RowKind {
        let running = card.running();
        let right = rect.right() - 8.0;
        let top = rect.y + 9.0;
        let mut buttons = Vec::new();
        if running && card.is_snowluma {
            buttons.push((CardButton::Novnc, Rect::new(right - 80.0, top, 24.0, 24.0)));
        }
        if running {
            buttons.push((CardButton::Webui, Rect::new(right - 52.0, top, 24.0, 24.0)));
        }
        buttons.push((
            CardButton::StartStop,
            Rect::new(right - 24.0, top, 24.0, 24.0),
        ));
        let left = buttons.first().map_or(right - 24.0, |(_, r)| r.x);
        // 状态点 h-2 w-2 mr-0.5，和按钮之间 gap-1：圆心在最左按钮左边 10
        let dot = (left - 10.0, rect.y + 21.0);
        let text_x = rect.x + 46.0;
        let text_w = (left - 14.0) - 8.0 - text_x;
        let avatar = Rect::new(rect.x + 8.0, rect.y + 6.0, 30.0, 30.0);
        let initial = centered(
            &initial(&card.name),
            TextStyle::sans(11.0, 600, 11.0),
            avatar,
        );
        RowKind::Card {
            index,
            avatar,
            initial,
            name: text(
                &card.name,
                TextStyle::sans(12.0, 500, 15.0),
                text_w,
                text_x,
                rect.y + 8.25,
            ),
            qq: text(
                &format!("QQ: {}", card.qq),
                TextStyle::mono(10.5, 400, 10.5),
                text_w,
                text_x,
                rect.y + 23.25,
            ),
            dot,
            buttons,
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hot> {
        let layout = self.layout.as_ref()?;
        for (i, row) in layout.rows.iter().enumerate() {
            match &row.kind {
                RowKind::Pager {
                    prev,
                    next,
                    prev_on,
                    next_on,
                    ..
                } => {
                    if *prev_on && prev.contains(x, y) {
                        return Some(Hot::Prev);
                    }
                    if *next_on && next.contains(x, y) {
                        return Some(Hot::Next);
                    }
                }
                RowKind::Card { buttons, .. } if row.rect.contains(x, y) => {
                    let button = buttons
                        .iter()
                        .find(|(_, r)| r.contains(x, y))
                        .map(|(b, _)| *b);
                    return Some(match button {
                        Some(b) => Hot::Button(i, b),
                        None => Hot::Row(i),
                    });
                }
                RowKind::Action { .. } if row.rect.contains(x, y) => return Some(Hot::Row(i)),
                _ => {}
            }
        }
        None
    }

    fn action_for(&self, hot: Hot) -> Option<Action> {
        let layout = self.layout.as_ref()?;
        match hot {
            Hot::Prev => (self.current_page() > 1).then_some(Action::PagePrev),
            Hot::Next => {
                (self.current_page() < self.snapshot.total_pages()).then_some(Action::PageNext)
            }
            Hot::Row(i) => match &layout.rows.get(i)?.kind {
                RowKind::Action { action, .. } => Some(*action),
                _ => None,
            },
            Hot::Button(i, button) => {
                let RowKind::Card { index, .. } = &layout.rows.get(i)?.kind else {
                    return None;
                };
                let card = self.snapshot.cards.get(*index)?;
                match button {
                    CardButton::StartStop => {
                        (!self.transitioning(card)).then_some(Action::StartStop(*index))
                    }
                    CardButton::Webui => (card.running() && card.webui.is_some())
                        .then_some(Action::OpenWebui(*index)),
                    CardButton::Novnc => {
                        (card.running() && card.is_snowluma).then_some(Action::OpenNovnc(*index))
                    }
                }
            }
        }
    }

    fn hover(&mut self, x: f32, y: f32, inside: bool) {
        self.hovered = if inside { self.hit(x, y) } else { None };
    }

    fn is_animating(&self) -> bool {
        !self.pending.is_empty()
            || self
                .snapshot
                .cards
                .get(self.visible())
                .unwrap_or_default()
                .iter()
                .any(|c| self.transitioning(c))
    }

    fn animate(&mut self, dt_ms: u32) -> bool {
        if !self.is_animating() {
            return false;
        }
        // 加载圈一秒一圈；状态点 animate-pulse 两秒一个来回
        self.spinner = (self.spinner + dt_ms as f32 * 0.36) % 360.0;
        self.pulse_ms = (self.pulse_ms + dt_ms) % 2000;
        true
    }

    fn pulse_alpha(&self) -> f32 {
        let t = self.pulse_ms as f32 / 2000.0;
        // tailwind animate-pulse：0 → 50% 变到 0.5，再回到 1
        1.0 - 0.5 * (1.0 - (2.0 * t - 1.0).abs())
    }

    fn paint(&self, canvas: &Canvas, theme: &Theme) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        for (i, row) in layout.rows.iter().enumerate() {
            match &row.kind {
                RowKind::Header { header, running } => {
                    match &self.logo {
                        Some(logo) => canvas.image_round(logo, HEADER_ICON, theme.radius_md),
                        None => canvas.fill_round(HEADER_ICON, theme.radius_md, theme.brand_soft),
                    }
                    canvas.ring(HEADER_ICON, theme.radius_md, theme.border_subtle.alpha(0.6));
                    let dot = if *running {
                        theme.success
                    } else {
                        theme.text_disabled
                    };
                    header.paint(canvas, theme, dot);
                }
                RowKind::Error { text } => {
                    canvas.fill_round(row.rect, theme.radius_md, theme.danger_bg);
                    draw(canvas, text, theme.danger);
                }
                RowKind::Separator => separator(canvas, theme, WIDTH, row.rect.y),
                RowKind::Pager {
                    label,
                    page,
                    prev,
                    next,
                    prev_on,
                    next_on,
                } => {
                    draw(canvas, label, theme.text_tertiary);
                    draw(canvas, page, theme.text_secondary);
                    for (rect, on, hot, icon) in [
                        (prev, *prev_on, Hot::Prev, Icon::ChevronLeft),
                        (next, *next_on, Hot::Next, Icon::ChevronRight),
                    ] {
                        let hovered = on && self.hovered == Some(hot);
                        if hovered {
                            canvas.fill_round(*rect, BUTTON_RADIUS, theme.brand_soft);
                        }
                        let color = if !on {
                            theme.text_secondary.alpha(0.3)
                        } else if hovered {
                            theme.brand
                        } else {
                            theme.text_secondary
                        };
                        canvas.icon(icon, rect.x + 3.5, rect.y + 3.5, 13.0, 2.0, color, false);
                    }
                }
                RowKind::Card { .. } => self.paint_card(canvas, theme, i, row),
                RowKind::Action { row, .. } => {
                    row.paint(canvas, theme, self.hovered == Some(Hot::Row(i)));
                }
            }
        }
    }

    fn paint_card(&self, canvas: &Canvas, theme: &Theme, i: usize, row: &Row) {
        let RowKind::Card {
            index,
            avatar,
            initial,
            name,
            qq,
            dot,
            buttons,
        } = &row.kind
        else {
            return;
        };
        let Some(card) = self.snapshot.cards.get(*index) else {
            return;
        };
        // WebView 写的是 bg-surface-muted/60，但 tailwind 主题里没有 --color-surface-muted，
        // 这个 class 实际不生效：卡片一直是面板底色只带一圈 ring，用户看惯的就是这样
        canvas.ring(row.rect, theme.radius_md, theme.border_subtle.alpha(0.5));

        match AvatarKey::user(&card.qq).and_then(|k| self.avatars.image(&k)) {
            Some(image) => canvas.image_round(&image, *avatar, theme.radius_md),
            None => {
                let (from, to) = avatar_palette(&card.qq);
                canvas.gradient_round(*avatar, theme.radius_md, from, to);
                draw(canvas, initial, Rgba::rgb(0xff, 0xff, 0xff).alpha(0.95));
            }
        }
        canvas.ring(*avatar, theme.radius_md, theme.border_subtle.alpha(0.7));
        draw(canvas, name, theme.text);
        draw(canvas, qq, theme.text_tertiary);

        let transitioning = self.transitioning(card);
        let dot_color = if card.running() {
            theme.success
        } else if transitioning {
            theme.brand.alpha(self.pulse_alpha())
        } else if card.state == "crashed" {
            theme.danger
        } else {
            theme.text_disabled
        };
        canvas.circle(dot.0, dot.1, 4.0, dot_color);

        for (button, rect) in buttons {
            let hovered = self.hovered == Some(Hot::Button(i, *button));
            match button {
                CardButton::StartStop if transitioning => {
                    canvas.icon_rotated(
                        Icon::LoaderCircle,
                        rect.x + 5.5,
                        rect.y + 5.5,
                        13.0,
                        2.0,
                        theme.brand,
                        self.spinner,
                    );
                }
                CardButton::StartStop => {
                    let stop = card.running();
                    let (bg, fg) = match (hovered, stop) {
                        (true, true) => (theme.danger_bg, theme.danger),
                        (true, false) => (theme.brand_soft, theme.brand),
                        _ => (Rgba::TRANSPARENT, theme.text_secondary),
                    };
                    canvas.fill_round(*rect, BUTTON_RADIUS, bg);
                    let icon = if stop { Icon::Square } else { Icon::Play };
                    canvas.icon(icon, rect.x + 6.5, rect.y + 6.5, 11.0, 2.0, fg, true);
                }
                CardButton::Webui | CardButton::Novnc => {
                    let (bg, fg) = if hovered {
                        (theme.brand_soft, theme.brand)
                    } else {
                        (Rgba::TRANSPARENT, theme.text_secondary)
                    };
                    canvas.fill_round(*rect, BUTTON_RADIUS, bg);
                    let icon = if *button == CardButton::Webui {
                        Icon::Globe
                    } else {
                        Icon::Monitor
                    };
                    canvas.icon(icon, rect.x + 5.5, rect.y + 5.5, 13.0, 1.9, fg, false);
                }
            }
        }
    }
}

fn action_row(width: f32, y: f32, icon: Icon, label: &str, action: Action) -> Row {
    let row = ActionRow::new(width, y, icon, label, 0.0);
    Row {
        rect: row.rect,
        kind: RowKind::Action { row, action },
    }
}

// ---------- 业务外壳：持有 AppHandle，执行动作、拉数据 ----------

pub struct TrayPanelNative {
    app: tauri::AppHandle,
    view: RefCell<TrayView>,
    panel: RefCell<Option<Rc<NativePanel>>>,
}

thread_local! {
    static CONTENT: RefCell<Option<Rc<TrayPanelNative>>> = const { RefCell::new(None) };
}

fn content() -> Option<Rc<TrayPanelNative>> {
    CONTENT.with(|slot| slot.borrow().clone())
}

fn with_content(f: impl FnOnce(&Rc<TrayPanelNative>)) {
    if let Some(content) = content() {
        f(&content);
    }
}

/// 打开面板前读一次 app-settings 的主题和圆角，WebView 版也是展开时从磁盘读的。
pub(crate) fn sync_ui_preferences(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let ui = ncd_runtime::desktop::load_app_settings(&state.data_root).ui_preferences;
    set_ui_preferences(UiPreferences {
        theme: ui.theme,
        radius_style: ui.radius_style,
    });
}

fn deliver_avatar(key: AvatarKey, pixels: Option<Arc<AvatarPixels>>) {
    with_content(|c| {
        if c.with_view(|v| v.avatars.fill(key, pixels)).is_some()
            && let Some(panel) = c.panel()
        {
            panel.invalidate();
        }
    });
}

impl TrayPanelNative {
    fn new(app: tauri::AppHandle) -> Self {
        let logo = crate::window_icon::main_window_icon(&app).ok().map(|img| {
            Rc::new(Image {
                width: img.width(),
                height: img.height(),
                rgba: img.rgba().to_vec(),
            })
        });
        Self {
            app,
            view: RefCell::new(TrayView::new(logo)),
            panel: RefCell::new(None),
        }
    }

    fn set_panel(&self, panel: Rc<NativePanel>) {
        if let Ok(mut slot) = self.panel.try_borrow_mut() {
            *slot = Some(panel);
        }
    }

    fn panel(&self) -> Option<Rc<NativePanel>> {
        self.panel.try_borrow().ok().and_then(|p| p.clone())
    }

    /// 视图借用只活在 f 里；f 不许回调 panel / do_action。借不到（重入）返回 None。
    fn with_view<R>(&self, f: impl FnOnce(&mut TrayView) -> R) -> Option<R> {
        self.view.try_borrow_mut().ok().map(|mut v| f(&mut v))
    }

    fn mark_dirty(&self) {
        self.with_view(|v| v.layout = None);
        if let Some(panel) = self.panel() {
            panel.resize_to_content();
        }
    }

    fn request_avatars(&self) {
        let keys = self.with_view(TrayView::claim_avatars).unwrap_or_default();
        tray_avatars::spawn_loads(&self.app, keys, deliver_avatar);
    }

    fn apply_snapshot(&self, snapshot: Snapshot) {
        self.with_view(|v| v.set_snapshot(snapshot));
        self.request_avatars();
        self.mark_dirty();
    }

    /// 先拿上一次的快照（第一次是空快照）立刻显示，免得用户觉得点了没反应；
    /// 数据回来后只重排：面板还开着就按锚点重新摆，已经收起的不会被弹回来。只从主线程调用。
    fn open_at(self: &Rc<Self>, anchor: (i32, i32)) {
        sync_ui_preferences(&self.app);
        self.with_view(|v| {
            v.page = 1;
            v.hovered = None;
            v.layout = None;
        });
        if let Some(panel) = self.panel() {
            panel.refresh_theme();
            panel.show_with(anchor, true, crate::native_panel::window::tray_panel_xy);
        }
        self.fetch();
    }

    fn fetch(&self) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = full_snapshot(&app).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|content| content.apply_snapshot(snapshot));
            });
        });
    }

    /// 面板开着的时候刷一轮数据。只从主线程调用。
    fn refresh_if_visible(&self) {
        if self.panel().is_some_and(|p| p.is_visible()) {
            self.fetch();
        }
    }

    fn do_action(self: &Rc<Self>, action: Action) {
        let app = self.app.clone();
        let card = |index: usize| {
            self.view
                .try_borrow()
                .ok()
                .and_then(|v| v.snapshot.cards.get(index).cloned())
        };
        match action {
            Action::ShowMain => {
                tauri::async_runtime::spawn(async move {
                    let _ = crate::commands::tray::window_show(app).await;
                });
            }
            Action::Lightweight => {
                tauri::async_runtime::spawn(async move {
                    let _ = crate::commands::tray::tray_panel_enter_lightweight(app).await;
                });
            }
            Action::Quit => {
                tauri::async_runtime::spawn(async move {
                    let _ = crate::commands::tray::tray_panel_quit(app).await;
                });
            }
            Action::StartStop(index) => {
                let Some(card) = card(index) else { return };
                let running_now = card.running();
                let bot_id = card.bot_id.clone();
                self.with_view(|v| {
                    v.pending.insert(bot_id.clone());
                    v.error = None;
                });
                self.mark_dirty();
                tauri::async_runtime::spawn(async move {
                    let result = start_stop(&app, &bot_id, running_now).await;
                    if let Err(error) = &result {
                        tracing::warn!(bot_id, error = %error, "native tray start/stop failed");
                    }
                    let failed = result.is_err();
                    let _ = app.run_on_main_thread(move || {
                        with_content(|content| {
                            content.with_view(|v| {
                                v.pending.remove(&bot_id);
                                if failed {
                                    let verb = if running_now { "停止" } else { "启动" };
                                    v.error = Some(format!("{verb}失败，详情见日志"));
                                }
                            });
                            content.mark_dirty();
                            content.fetch();
                        });
                    });
                });
            }
            Action::OpenWebui(index) => {
                let Some(card) = card(index) else { return };
                tauri::async_runtime::spawn(async move {
                    open_napcat_webui(&app, &card.bot_id).await;
                });
            }
            Action::OpenNovnc(index) => {
                let Some(card) = card(index) else { return };
                tauri::async_runtime::spawn(async move {
                    open_snowluma_novnc(&app, &card.bot_id).await;
                });
            }
            Action::PagePrev | Action::PageNext => {
                let changed = self.with_view(|v| {
                    let pages = v.snapshot.total_pages();
                    let current = v.current_page();
                    let next = if action == Action::PagePrev {
                        current.saturating_sub(1).max(1)
                    } else {
                        (current + 1).min(pages)
                    };
                    v.page = next;
                    v.hovered = None;
                    next != current
                });
                if changed == Some(true) {
                    self.request_avatars();
                    self.mark_dirty();
                }
            }
        }
    }
}

async fn start_stop(app: &tauri::AppHandle, bot_id: &str, running_now: bool) -> Result<(), String> {
    let state = app
        .try_state::<crate::AppState>()
        .ok_or_else(|| "app state not ready".to_string())?;
    let id = ncd_domain::BotId::new(bot_id.to_string());
    if running_now {
        state
            .bot_manager
            .stop_bot(&id)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        crate::commands::bot::ensure_desktop_consent(&state)?;
        state.migrate_gate.ensure_idle()?;
        state
            .bot_manager
            .start_bot(&id)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// 把 Rc<TrayPanelNative> 包成 Box<dyn PanelContent>。
pub struct TrayPanelCell(pub Rc<TrayPanelNative>);

impl PanelContent for TrayPanelCell {
    fn measure(&mut self, width: f32, _theme: &Theme) -> (f32, f32) {
        let width = if width <= 0.0 { WIDTH } else { width };
        let height = self.0.with_view(|v| v.layout(width)).unwrap_or(137.5);
        (WIDTH, height)
    }

    fn paint(&mut self, canvas: &Canvas, _x: f32, _y: f32, width: f32, theme: &Theme) {
        self.0.with_view(|v| {
            if v.layout.is_none() {
                v.layout(width);
            }
            v.paint(canvas, theme);
        });
    }

    fn click(&mut self, x: f32, y: f32, _theme: &Theme) -> bool {
        let action = self
            .0
            .with_view(|v| v.hit(x, y).and_then(|hot| v.action_for(hot)))
            .flatten();
        let Some(action) = action else { return false };
        self.0.do_action(action);
        matches!(
            action,
            Action::ShowMain | Action::Lightweight | Action::Quit
        )
    }

    fn hover(&mut self, x: f32, y: f32, inside: bool, _theme: &Theme) {
        self.0.with_view(|v| v.hover(x, y, inside));
    }

    fn key(&mut self, vk: u16, _theme: &Theme) -> bool {
        let action = match vk {
            0x25 => Action::PagePrev,
            0x27 => Action::PageNext,
            _ => return false,
        };
        self.0.do_action(action);
        true
    }

    fn animate(&mut self, dt_ms: u32, _theme: &Theme) -> bool {
        self.0.with_view(|v| v.animate(dt_ms)).unwrap_or(false)
    }

    fn is_animating(&self) -> bool {
        self.0
            .view
            .try_borrow()
            .map(|v| v.is_animating())
            .unwrap_or(false)
    }
}

// ---------- 后台数据 ----------

async fn full_snapshot(app: &tauri::AppHandle) -> Snapshot {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return Snapshot::default();
    };
    let snapshots = state.bot_manager.list_snapshots().await;
    // bot_id 就是 QQ 号（BotManager::get_bot_config 按它 parse 出 qq_id），所以按 qq_id 建表
    let configs: HashMap<String, ncd_domain::BotConfig> = state
        .bot_manager
        .list_bot_configs()
        .await
        .map(|v| {
            v.into_iter()
                .map(|c| (c.bot.qq_id.to_string(), c))
                .collect()
        })
        .unwrap_or_default();
    let flavors: HashMap<String, ncd_domain::bot_config::BackendType> = state
        .bot_manager
        .list_bot_flavors()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(id, backend)| (id.to_string(), backend))
        .collect();
    let webuis: HashMap<String, (u16, String)> = state
        .bot_manager
        .list_napcat_webui_endpoints()
        .await
        .into_iter()
        .map(|(id, port, token, _online)| (id.to_string(), (port, token)))
        .collect();
    let cards = snapshots
        .into_iter()
        .map(|snap| {
            let bot_id = snap.bot_id.to_string();
            let cfg = configs.get(&bot_id);
            let is_snowluma = matches!(
                flavors.get(&bot_id),
                Some(ncd_domain::bot_config::BackendType::SnowLuma)
            ) || cfg.is_some_and(|c| {
                matches!(
                    c.bot.backend_type,
                    ncd_domain::bot_config::BackendType::SnowLuma
                )
            });
            BotCard {
                state: BotCard::state_str(&snap.state).to_string(),
                name: cfg
                    .map(|c| c.bot.name.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| bot_id.clone()),
                qq: cfg
                    .map(|c| c.bot.qq_id.to_string())
                    .filter(|q| !q.is_empty() && q != "0")
                    .unwrap_or_else(|| bot_id.clone()),
                is_snowluma,
                webui: webuis.get(&bot_id).cloned(),
                bot_id,
            }
        })
        .collect();
    Snapshot { cards }
}

// ---------- URL / noVNC ----------

async fn open_napcat_webui(app: &tauri::AppHandle, bot_id: &str) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let bindings = state.bot_manager.list_napcat_webui_endpoints().await;
    let Some((_, port, token, _)) = bindings
        .into_iter()
        .find(|(id, _, _, _)| id.as_str() == bot_id)
    else {
        return;
    };
    let url = format!(
        "http://127.0.0.1:{port}/webui?token={}",
        urlencoding::encode(&token)
    );
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app.opener().open_url(&url, None::<&str>) {
        tracing::warn!("native tray open webui: {e}");
    }
}

async fn open_snowluma_novnc(app: &tauri::AppHandle, bot_id: &str) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let bid = ncd_domain::BotId::new(bot_id.to_string());
    let Ok(Some(cfg)) = state.bot_manager.get_bot_config(&bid).await else {
        return;
    };
    if cfg.bot.backend_type != ncd_domain::BackendType::SnowLuma {
        return;
    }
    let url = if cfg.bot.deployment_type == ncd_domain::DeploymentType::Docker {
        let Some(ep) = state.bot_manager.snowluma_docker_endpoints(&bid).await else {
            return;
        };
        let pwd = urlencoding::encode(&ep.vnc_password);
        format!(
            "http://127.0.0.1:{}/vnc.html?autoconnect=1&resize=scale&password={pwd}&view_only=0&reconnect=1&reconnect_delay=3000",
            ep.novnc_local_port
        )
    } else if cfg.bot.deployment_type == ncd_domain::DeploymentType::Native {
        let ncd_domain::RuntimeTarget::Server(server_id) = &cfg.bot.runtime_target else {
            return;
        };
        let Some(ep) = state
            .bot_manager
            .snowluma_native_endpoints_for_server(server_id)
            .await
        else {
            return;
        };
        let pwd = urlencoding::encode(&ep.vnc_password);
        format!(
            "http://127.0.0.1:{}/vnc.html?autoconnect=1&resize=scale&password={pwd}&view_only=0&reconnect=1&reconnect_delay=3000",
            ep.novnc_local_port
        )
    } else {
        return;
    };
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app.opener().open_url(&url, None::<&str>) {
        tracing::warn!("native tray open novnc: {e}");
    }
}

// ---------- 闲置回收 ----------

const RELEASE_AFTER: std::time::Duration = std::time::Duration::from_secs(30);
static RELEASE_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 收着的主托盘面板连窗口一起销毁。返回 false 表示面板还开着。只从主线程调。
pub(crate) fn release_if_hidden() -> bool {
    let Some(content) = content() else {
        return true;
    };
    if content.panel().is_some_and(|p| p.is_visible()) {
        return false;
    }
    if let Some(panel) = content.panel() {
        panel.destroy();
    }
    CONTENT.with(|slot| slot.borrow_mut().take());
    true
}

/// 每次打开面板后叫：两个托盘面板都收着满 30 秒，就销毁窗口并放掉 D2D / DirectWrite，
/// 不然第一次右键后主进程一直多背十几 MB。再次打开会顺延计时。
pub(crate) fn schedule_release(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    let ticket = RELEASE_TICKET.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(RELEASE_AFTER).await;
            if RELEASE_TICKET.load(Ordering::Relaxed) != ticket {
                return;
            }
            let (send, receive) = tokio::sync::oneshot::channel();
            let posted = app.run_on_main_thread(move || {
                let main = release_if_hidden();
                let chat = crate::chat_tray_panel_native::release_if_hidden();
                if main && chat {
                    crate::native_panel::gfx::release_shared();
                }
                let _ = send.send(main && chat);
            });
            if posted.is_err() || receive.await.unwrap_or(true) {
                return;
            }
        }
    });
}

// ---------- 对外入口：open / refresh_if_visible / sync_theme ----------

/// 托盘右键入口。任意线程可调；内部把创建与打开都切到主线程。
pub fn open(app: &tauri::AppHandle, anchor: (i32, i32)) {
    schedule_release(app);
    let app_owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(content) = content() {
            content.open_at(anchor);
            return;
        }
        sync_ui_preferences(&app_owned);
        let content = Rc::new(TrayPanelNative::new(app_owned.clone()));
        let cell = Box::new(TrayPanelCell(content.clone())) as Box<dyn PanelContent>;
        match NativePanel::create(anchor, WIDTH, cell) {
            Ok(panel) => {
                content.set_panel(Rc::new(panel));
                CONTENT.with(|slot| *slot.borrow_mut() = Some(content.clone()));
                content.open_at(anchor);
            }
            Err(e) => tracing::warn!("native tray create: {e}"),
        }
    });
}

/// Bot 事件到达时叫；开着面板才刷新，收着什么都不做。
pub fn refresh_if_visible(app: &tauri::AppHandle) {
    let _ = app.run_on_main_thread(move || {
        with_content(|content| content.refresh_if_visible());
    });
}

/// 主窗把面板配色推过来时叫；下次 paint 用新主题。
pub fn sync_theme(app: &tauri::AppHandle) {
    let _ = app.run_on_main_thread(move || {
        with_content(|content| {
            if let Some(panel) = content.panel() {
                panel.refresh_theme();
            }
            content.mark_dirty();
        });
    });
}

#[cfg(all(test, windows))]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn card(id: &str, name: &str, state: &str, snowluma: bool) -> BotCard {
        BotCard {
            bot_id: id.into(),
            state: state.into(),
            name: name.into(),
            qq: id.into(),
            is_snowluma: snowluma,
            webui: (state == "running").then(|| (6099, "t".into())),
        }
    }

    pub(crate) fn view_with(cards: Vec<BotCard>) -> TrayView {
        let mut v = TrayView::new(None);
        v.set_snapshot(Snapshot { cards });
        v
    }

    fn texts(v: &TrayView) -> Vec<(f32, f32, f32, f32)> {
        let mut out = Vec::new();
        let push = |t: &Option<Text>, out: &mut Vec<_>| {
            if let Some(t) = t {
                out.push((t.x, t.y, t.tb.width, t.tb.height));
            }
        };
        let Some(layout) = v.layout.as_ref() else {
            return out;
        };
        for row in &layout.rows {
            match &row.kind {
                RowKind::Header { header, .. } => {
                    push(&header.title, &mut out);
                    push(&header.status, &mut out);
                }
                RowKind::Error { text } => push(text, &mut out),
                RowKind::Pager { label, page, .. } => {
                    push(label, &mut out);
                    push(page, &mut out);
                }
                RowKind::Card { name, qq, .. } => {
                    push(name, &mut out);
                    push(qq, &mut out);
                }
                RowKind::Action { row, .. } => push(&row.label, &mut out),
                RowKind::Separator => {}
            }
        }
        out
    }

    #[test]
    fn heights_match_webview_box_model() {
        assert!((view_with(vec![]).layout(WIDTH) - 137.5).abs() < 0.01);
        let one = vec![card("10001", "a", "stopped", false)];
        assert!((view_with(one).layout(WIDTH) - 192.5).abs() < 0.01);
        let two = vec![
            card("10001", "a", "stopped", false),
            card("10002", "b", "running", false),
        ];
        assert!((view_with(two).layout(WIDTH) - 238.5).abs() < 0.01);
        let three = vec![
            card("10001", "a", "stopped", false),
            card("10002", "b", "running", false),
            card("10003", "c", "stopped", false),
        ];
        assert!((view_with(three).layout(WIDTH) - 262.5).abs() < 0.01);
        let mut err = view_with(vec![]);
        err.error = Some("启动失败，详情见日志".into());
        assert!((err.layout(WIDTH) - (137.5 + 11.0 * 1.375 + 8.0 + 4.0)).abs() < 0.01);
    }

    #[test]
    fn header_and_pager_do_not_overlap() {
        let mut v = view_with(vec![
            card("2550419068", "2550419068", "running", true),
            card(
                "2707600964",
                "超长的名字超长的名字超长的名字超长的名字",
                "stopped",
                false,
            ),
            card("10003", "c", "stopped", false),
        ]);
        v.layout(WIDTH);
        let layout = v.layout.as_ref().unwrap();
        for row in &layout.rows {
            match &row.kind {
                RowKind::Header { header, .. } => {
                    let title = header.title.as_ref().unwrap();
                    let status = header.status.as_ref().unwrap();
                    assert!(
                        title.y + title.tb.height <= status.y + 0.01,
                        "title above status"
                    );
                    assert!(status.x > 52.0, "status text clears the dot");
                }
                RowKind::Pager {
                    label,
                    page,
                    prev,
                    next,
                    ..
                } => {
                    let page = page.as_ref().unwrap();
                    assert!(prev.right() + 6.0 <= page.x + 0.01);
                    assert!(page.right() + 6.0 <= next.x + 0.01, "page text clears ›");
                    assert!(label.as_ref().unwrap().right() <= prev.x + 0.01);
                }
                RowKind::Card {
                    name,
                    qq,
                    dot,
                    buttons,
                    ..
                } => {
                    for t in [name, qq] {
                        assert!(
                            t.as_ref().unwrap().right() < dot.0 - 4.0,
                            "text clears the status dot"
                        );
                    }
                    for (i, (_, a)) in buttons.iter().enumerate() {
                        for (_, b) in buttons.iter().skip(i + 1) {
                            assert!(a.right() <= b.x, "buttons do not overlap");
                        }
                    }
                }
                _ => {}
            }
        }
        for (x, _, w, _) in texts(&v) {
            assert!(x >= 0.0 && x + w <= WIDTH + 0.01);
        }
    }

    #[test]
    fn hit_maps_buttons_and_pager() {
        let mut v = view_with(vec![
            card("10001", "a", "running", true),
            card("10002", "b", "stopped", false),
            card("10003", "c", "stopped", false),
        ]);
        v.layout(WIDTH);
        // 第一张卡片在 y = 39.5 + 9 + 24 + 2；启停按钮贴右
        let card_y = 39.5 + 9.0 + 24.0 + 2.0;
        let hot = v.hit(8.0 + 236.0 - 12.0, card_y + 21.0).unwrap();
        assert_eq!(v.action_for(hot), Some(Action::StartStop(0)));
        let hot = v.hit(8.0 + 236.0 - 40.0, card_y + 21.0).unwrap();
        assert_eq!(v.action_for(hot), Some(Action::OpenWebui(0)));
        let hot = v.hit(8.0 + 236.0 - 68.0, card_y + 21.0).unwrap();
        assert_eq!(v.action_for(hot), Some(Action::OpenNovnc(0)));
        // 卡片空白处只是 hover，不触发动作
        let hot = v.hit(120.0, card_y + 21.0).unwrap();
        assert_eq!(v.action_for(hot), None);
        let hot = v.hit(240.0, 39.5 + 9.0 + 10.0).unwrap();
        assert_eq!(v.action_for(hot), Some(Action::PageNext));
        assert_eq!(
            v.hit(222.0 - 40.0, 39.5 + 9.0 + 10.0),
            None,
            "‹ disabled on page 1"
        );
    }

    /// 测试截图用：直接把视图包成面板内容，不碰 AppHandle。
    pub(crate) struct Shot(pub TrayView);

    impl PanelContent for Shot {
        fn measure(&mut self, width: f32, _theme: &Theme) -> (f32, f32) {
            (WIDTH, self.0.layout(width))
        }
        fn paint(&mut self, canvas: &Canvas, _x: f32, _y: f32, width: f32, theme: &Theme) {
            if self.0.layout.is_none() {
                self.0.layout(width);
            }
            self.0.paint(canvas, theme);
        }
        fn click(&mut self, _x: f32, _y: f32, _theme: &Theme) -> bool {
            false
        }
        fn hover(&mut self, x: f32, y: f32, inside: bool, _theme: &Theme) {
            self.0.hover(x, y, inside);
        }
        fn key(&mut self, _vk: u16, _theme: &Theme) -> bool {
            false
        }
        fn animate(&mut self, _dt_ms: u32, _theme: &Theme) -> bool {
            false
        }
    }

    pub(crate) fn fake_avatar(seed: u8) -> Arc<AvatarPixels> {
        let mut rgba = Vec::with_capacity(128 * 128 * 4);
        for y in 0..128u32 {
            for x in 0..128u32 {
                let d = ((x as i32 - 64).pow(2) + (y as i32 - 52).pow(2)) as f32;
                let face = d < 900.0;
                let (r, g, b) = if face {
                    (250, 220, 190)
                } else {
                    (
                        seed.wrapping_mul(40),
                        120 + (y / 2) as u8,
                        200 - (x / 2) as u8,
                    )
                };
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Arc::new(AvatarPixels { size: 128, rgba })
    }

    fn logo() -> Option<Rc<Image>> {
        let img = image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/256x256.png"))
            .ok()?
            .into_rgba8();
        Some(Rc::new(Image {
            width: img.width(),
            height: img.height(),
            rgba: img.into_raw(),
        }))
    }

    fn with_avatars(mut v: TrayView, ids: &[&str]) -> TrayView {
        for (i, id) in ids.iter().enumerate() {
            if let Some(key) = AvatarKey::user(id) {
                v.avatars.fill(key, Some(fake_avatar(i as u8 + 1)));
            }
        }
        v.logo = logo();
        v
    }

    fn shots(name: &str, make: impl Fn() -> TrayView) {
        for (scale, pct) in [(1.0, 100), (1.25, 125), (1.5, 150)] {
            let mut v = make();
            let h = v.layout(WIDTH);
            crate::native_panel::shots::offscreen(&format!("{name}-{pct}"), scale, h, |c| {
                v.paint(c, &crate::native_panel::theme::theme());
            });
        }
    }

    #[test]
    #[ignore = "需要 NCD_PANEL_SHOTS_DIR 和交互桌面"]
    fn render_panel_shots() {
        if crate::native_panel::shots::dir().is_none() {
            return;
        }
        set_ui_preferences(UiPreferences {
            theme: "latte".into(),
            radius_style: "square".into(),
        });
        // 和用户截图同一个状态：3 个 Bot 全停、第 1/2 页
        let reference = || {
            with_avatars(
                view_with(vec![
                    card("2707600964", "AQiaoYO", "stopped", false),
                    card("572381217", "Bot-1217", "stopped", false),
                    card("10003", "c", "stopped", false),
                ]),
                &["2707600964", "572381217"],
            )
        };
        shots("native-main-ref", reference);
        // 运行中的 SnowLuma、超长名字、切换中、错误条、hover 在第二张卡片的地球按钮上
        let busy = || {
            let mut v = view_with(vec![
                card(
                    "2550419068",
                    "SnowLuma 运行中的一个特别特别长的机器人名字",
                    "running",
                    true,
                ),
                card("2707600964", "AQiaoYO", "running", false),
                card("10003", "c", "starting", false),
            ]);
            v.pending.insert("10003".into());
            v.error = Some("启动失败，详情见日志".into());
            v.layout(WIDTH);
            let card_y = 39.5 + 23.125 + 4.0 + 9.0 + 24.0 + 2.0 + 46.0;
            v.hover(8.0 + 236.0 - 40.0, card_y + 21.0, true);
            with_avatars(v, &["2707600964"])
        };
        shots("native-main-busy", busy);
        let empty = || {
            let mut v = with_avatars(view_with(vec![]), &[]);
            v.layout(WIDTH);
            v.hover(100.0, 39.5 + 5.0 + 2.0 + 10.0, true);
            v
        };
        shots("native-main-empty", empty);

        crate::native_panel::shots::window(
            "native-main-window-latte",
            Box::new(Shot(reference())),
            (1400, 1100),
            crate::native_panel::window::tray_panel_xy,
        );
        set_ui_preferences(UiPreferences {
            theme: "mocha".into(),
            radius_style: "standard".into(),
        });
        crate::native_panel::shots::window(
            "native-main-window-mocha",
            Box::new(Shot(reference())),
            (1400, 1100),
            crate::native_panel::window::tray_panel_xy,
        );
        shots("native-main-mocha", reference);
    }

    #[test]
    fn avatar_palette_matches_js_hash() {
        // JS: let h = 0; for (c of '2707600964') h = (h * 31 + c.charCodeAt(0)) | 0; Math.abs(h) % 6
        let h = "2707600964"
            .encode_utf16()
            .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)));
        assert_eq!(
            avatar_palette("2707600964"),
            PALETTE[(h.unsigned_abs() % 6) as usize]
        );
        assert_eq!(initial("  aqiao"), "A");
        assert_eq!(initial(""), "?");
    }
}
