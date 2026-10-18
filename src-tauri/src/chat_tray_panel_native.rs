// 账号托盘面板（原生 Direct2D）：右键菜单 + 悬停未读预览。
#![warn(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::Manager;

use crate::avatar_cache::{AvatarKey, AvatarPixels};
use crate::native_panel::gfx::{Canvas, Rect, TextStyle};
use crate::native_panel::icons::Icon;
use crate::native_panel::parts::{
    ACTION_H, ActionRow, Badge, HEADER_H, HEADER_ICON, Header, SEPARATOR_H, Text, centered, draw,
    initial, separator, text,
};
use crate::native_panel::theme::Theme;
use crate::native_panel::window::{NativePanel, PanelContent};
use crate::tray_avatars::{self, AvatarSlots};

const WIDTH: f32 = 260.0;
// .chat-tray-conversations 的 max-height 284：5 行（240）放得下，第 6 行就要滚动，原生版不做滚动
const MAX_ROWS: usize = 5;
const MIN_HEIGHT: f32 = 192.0;

#[derive(Clone)]
pub struct ConvItem {
    pub key: String,
    pub name: String,
    pub preview: String,
    pub unread: u32,
    pub id: String,
    pub is_group: bool,
}

impl ConvItem {
    fn avatar_key(&self) -> Option<AvatarKey> {
        if self.is_group {
            AvatarKey::group(&self.id)
        } else {
            AvatarKey::user(&self.id)
        }
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub target_name: String,
    pub self_id: String,
    pub status: String,
    pub online: bool,
    pub background: bool,
    pub notification_unread: u32,
    pub conversations: Vec<ConvItem>,
    pub conversation_count: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Open,
    Console,
    Background,
    HideTray,
    OpenConv(usize),
    DismissError,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Menu,
    Preview,
}

enum Trailing {
    None,
    Badge(Badge),
    Check,
}

enum RowKind {
    Header {
        header: Header,
        initial: Option<Text>,
    },
    Separator,
    Menu {
        row: ActionRow,
        trailing: Trailing,
    },
    Heading {
        label: Option<Text>,
        trailing: Option<Text>,
    },
    Conversation {
        index: usize,
        avatar: Rect,
        initial: Option<Text>,
        name: Option<Text>,
        preview: Option<Text>,
        badge: Badge,
    },
    Empty {
        label: Option<Text>,
    },
    Error {
        text: Option<Text>,
    },
}

struct Row {
    rect: Rect,
    kind: RowKind,
    action: Option<Action>,
}

struct Layout {
    rows: Vec<Row>,
    height: f32,
}

struct ModeTransition {
    start: Instant,
    duration_ms: u32,
}

impl ModeTransition {
    fn progress(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.start).as_secs_f32() * 1000.0
            / self.duration_ms.max(1) as f32)
            .min(1.0);
        1.0 - (1.0 - t).powi(3)
    }
}

/// 纯视图：不碰 AppHandle，测试直接构造。
pub(crate) struct ChatTrayView {
    mode: Mode,
    snapshot: Snapshot,
    error: Option<String>,
    layout: Option<Layout>,
    hovered: Option<usize>,
    // hovered 是键盘移过去的：会话行要画 focus-visible 的 brand-soft 底
    keyboard: bool,
    avatars: AvatarSlots,
    mode_transition: Option<ModeTransition>,
}

impl ChatTrayView {
    pub(crate) fn new(mode: Mode) -> Self {
        Self {
            mode,
            snapshot: Snapshot::default(),
            error: None,
            layout: None,
            hovered: None,
            keyboard: false,
            avatars: AvatarSlots::default(),
            mode_transition: None,
        }
    }

    fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
        self.error = None;
        self.layout = None;
    }

    fn switch_mode(&mut self, mode: Mode, animate: bool) {
        if self.mode == mode {
            return;
        }
        let duration_ms = crate::native_panel::window::transition_duration();
        let previous = self.layout.take();
        self.mode_transition =
            previous
                .filter(|_| animate && duration_ms > 0)
                .map(|_| ModeTransition {
                    start: Instant::now(),
                    duration_ms,
                });
        self.mode = mode;
        self.hovered = None;
        self.keyboard = false;
    }

    fn frame_height(&self) -> f32 {
        self.layout
            .as_ref()
            .map_or_else(|| self.shared_height(), |l| l.height)
    }

    fn shared_height(&self) -> f32 {
        (104.0 + 48.0 * self.shown_conversations().len() as f32).max(MIN_HEIGHT)
    }

    fn mode_changing(&self) -> bool {
        self.mode_transition
            .as_ref()
            .is_some_and(|t| t.progress(Instant::now()) < 1.0)
    }

    fn shown_conversations(&self) -> &[ConvItem] {
        let n = self.snapshot.conversations.len().min(MAX_ROWS);
        self.snapshot.conversations.get(..n).unwrap_or_default()
    }

    fn claim_avatars(&mut self) -> Vec<AvatarKey> {
        let mut wanted: Vec<AvatarKey> = AvatarKey::user(&self.snapshot.self_id)
            .into_iter()
            .collect();
        if self.mode == Mode::Preview {
            wanted.extend(
                self.shown_conversations()
                    .iter()
                    .filter_map(ConvItem::avatar_key),
            );
        }
        let keep: HashSet<AvatarKey> = wanted.iter().cloned().collect();
        self.avatars.retain(&keep);
        self.avatars.claim(wanted)
    }

    fn layout(&mut self, width: f32) -> f32 {
        let height = self.shared_height();
        let s = &self.snapshot;
        let mut rows = Vec::new();
        rows.push(Row {
            rect: Rect::new(0.0, 0.0, width, HEADER_H),
            kind: RowKind::Header {
                header: Header::new(
                    width,
                    &s.target_name,
                    &format!("{} · QQ: {}", s.status, s.self_id),
                ),
                initial: centered(
                    &initial(&s.target_name),
                    TextStyle::sans(11.0, 600, 11.0),
                    HEADER_ICON,
                ),
            },
            action: None,
        });
        let mut y = HEADER_H;
        y = push_separator(&mut rows, width, y);

        match self.mode {
            Mode::Menu => {
                let action_h = ((height - 68.5) / 4.0).clamp(ACTION_H, 44.0);
                let group_inset = (height - 68.5 - 4.0 * action_h) / 6.0;
                y += 2.0 + group_inset;
                let open = ActionRow::new(width, y, Icon::MessageCircle, "打开聊天", 0.0)
                    .with_height(action_h);
                let row = if s.notification_unread > 0 {
                    let badge = Badge::new(
                        s.notification_unread,
                        open.trailing_right(),
                        y + action_h / 2.0,
                    );
                    // 角标宽度量出来之后再按它给文字让位
                    let open =
                        ActionRow::new(width, y, Icon::MessageCircle, "打开聊天", badge.rect.w)
                            .with_height(action_h);
                    menu_row(open, Trailing::Badge(badge), Action::Open)
                } else {
                    menu_row(open, Trailing::None, Action::Open)
                };
                rows.push(row);
                y += action_h;
                rows.push(menu_row(
                    ActionRow::new(width, y, Icon::Monitor, "打开控制台", 0.0)
                        .with_height(action_h),
                    Trailing::None,
                    Action::Console,
                ));
                y += action_h + 2.0 + group_inset;
                y = push_separator(&mut rows, width, y);
                y += 2.0 + group_inset;
                let check = if s.background {
                    Trailing::Check
                } else {
                    Trailing::None
                };
                rows.push(menu_row(
                    ActionRow::new(width, y, Icon::Radio, "后台接收消息", 14.0)
                        .with_height(action_h),
                    check,
                    Action::Background,
                ));
                y += action_h + 2.0 + group_inset;
                y = push_separator(&mut rows, width, y);
                y += 2.0 + group_inset;
                rows.push(menu_row(
                    ActionRow::new(width, y, Icon::EyeOff, "隐藏此账号托盘", 0.0)
                        .with_height(action_h),
                    Trailing::None,
                    Action::HideTray,
                ));
            }
            Mode::Preview => {
                // .chat-tray-heading：padding 2px 10px 4px，11px，行高 1.5
                let heading = TextStyle::sans(11.0, 400, 16.5);
                let trailing = (s.conversation_count > 0)
                    .then(|| {
                        text(
                            &format!("{} 个会话", s.conversation_count),
                            heading,
                            120.0,
                            0.0,
                            y + 2.0,
                        )
                    })
                    .flatten()
                    .map(|mut t| {
                        t.x = width - 10.0 - t.tb.width;
                        t
                    });
                let label_w = trailing.as_ref().map_or(width - 20.0, |t| t.x - 10.0 - 8.0);
                rows.push(Row {
                    rect: Rect::new(0.0, y, width, 22.5),
                    kind: RowKind::Heading {
                        label: text("未读消息", heading, label_w, 10.0, y + 2.0),
                        trailing,
                    },
                    action: None,
                });
                y += 22.5;

                let shown = self.shown_conversations().len();
                if shown == 0 {
                    // .chat-tray-empty：min-height 88，Bell 23 + gap 8 + 一行字垂直居中
                    let label = centered(
                        "暂无新消息",
                        heading,
                        Rect::new(0.0, y + 51.25, width, 16.5),
                    );
                    rows.push(Row {
                        rect: Rect::new(0.0, y, width, 88.0),
                        kind: RowKind::Empty { label },
                        action: None,
                    });
                } else {
                    // .chat-tray-conversations：padding 2px 8px，gap 4；每行 min-height 44
                    y += 2.0;
                    for index in 0..shown {
                        let Some(conv) = s.conversations.get(index) else {
                            continue;
                        };
                        let rect = Rect::new(8.0, y, width - 16.0, 44.0);
                        rows.push(Row {
                            rect,
                            kind: conversation_kind(index, conv, rect),
                            action: Some(Action::OpenConv(index)),
                        });
                        y += 44.0;
                        if index + 1 < shown {
                            y += 4.0;
                        }
                    }
                }
                y = height - SEPARATOR_H - 2.0 - ACTION_H - 4.0;
                y = push_separator(&mut rows, width, y);
                y += 2.0;
                let label = if s.conversation_count > shown {
                    "查看全部会话"
                } else {
                    "打开聊天"
                };
                rows.push(menu_row(
                    ActionRow::new(width, y, Icon::MessageCircle, label, 0.0),
                    Trailing::None,
                    Action::Open,
                ));
            }
        }

        // 外框直接取共同尺寸；分组间距的浮点累加会让 ceil 多出一个物理像素。
        y = height;
        if let Some(err) = self.error.as_deref().filter(|e| !e.is_empty()) {
            // mx-2 mb-1 px-2 py-1 leading-snug，右边 X 按钮（padding 2 + 13）和文字隔 5
            let rect = Rect::new(8.0, y, width - 16.0, 25.0);
            rows.push(Row {
                rect,
                kind: RowKind::Error {
                    text: text(
                        err,
                        TextStyle::sans(11.0, 400, 11.0 * 1.375),
                        rect.w - 16.0 - 5.0 - 17.0,
                        rect.x + 8.0,
                        y + 4.0,
                    ),
                },
                action: Some(Action::DismissError),
            });
            y += rect.h + 4.0;
        }

        self.layout = Some(Layout { rows, height: y });
        y
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        if self.mode_changing() {
            return None;
        }
        self.layout
            .as_ref()?
            .rows
            .iter()
            .position(|row| row.action.is_some() && row.rect.contains(x, y))
    }

    fn action_at(&self, index: usize) -> Option<Action> {
        self.layout.as_ref()?.rows.get(index)?.action
    }

    fn hover(&mut self, x: f32, y: f32, inside: bool) {
        let next = if inside { self.hit(x, y) } else { None };
        if next.is_some() || !self.keyboard {
            self.hovered = next;
            self.keyboard = false;
        }
    }

    /// 上下 / Home / End 在所有能点的行之间循环，和 WebView 的 data-tray-item 一样；打开时不预选。
    fn key(&mut self, vk: u16) -> bool {
        if self.mode_changing() {
            return false;
        }
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let items: Vec<usize> = layout
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.action.is_some_and(|a| a != Action::DismissError))
            .map(|(i, _)| i)
            .collect();
        let (Some(first), Some(last)) = (items.first().copied(), items.last().copied()) else {
            return false;
        };
        let at = self
            .hovered
            .and_then(|h| items.iter().position(|&i| i == h));
        let next = match (vk, at) {
            (0x24, _) => first,
            (0x23, _) => last,
            (0x26, None) => last,
            (0x28, None) => first,
            (0x26, Some(p)) => p
                .checked_sub(1)
                .and_then(|p| items.get(p).copied())
                .unwrap_or(last),
            (0x28, Some(p)) => items.get(p + 1).copied().unwrap_or(first),
            _ => return false,
        };
        self.hovered = Some(next);
        self.keyboard = true;
        true
    }

    fn paint(&self, canvas: &Canvas, theme: &Theme) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        self.paint_layout(layout, canvas, theme);
    }

    fn paint_layout(&self, layout: &Layout, canvas: &Canvas, theme: &Theme) {
        for (i, row) in layout.rows.iter().enumerate() {
            let hot = self.hovered == Some(i);
            match &row.kind {
                RowKind::Header { header, initial } => {
                    let key = AvatarKey::user(&self.snapshot.self_id);
                    match key.and_then(|k| self.avatars.image(&k)) {
                        Some(image) => canvas.image_round(&image, HEADER_ICON, theme.radius_md),
                        None => {
                            canvas.fill_round(HEADER_ICON, theme.radius_md, theme.brand_soft);
                            draw(canvas, initial, theme.brand);
                        }
                    }
                    canvas.ring(HEADER_ICON, theme.radius_md, theme.border_subtle);
                    let dot = if self.snapshot.online {
                        theme.success
                    } else {
                        theme.text_disabled
                    };
                    header.paint(canvas, theme, dot);
                }
                RowKind::Separator => separator(canvas, theme, WIDTH, row.rect.y),
                RowKind::Menu {
                    row: action,
                    trailing,
                } => {
                    action.paint(canvas, theme, hot);
                    match trailing {
                        Trailing::None => {}
                        Trailing::Badge(badge) => badge.paint(canvas, theme),
                        Trailing::Check => canvas.icon(
                            Icon::Check,
                            action.trailing_right() - 14.0,
                            action.rect.y + (action.rect.h - 14.0) / 2.0,
                            14.0,
                            1.9,
                            theme.brand,
                            false,
                        ),
                    }
                }
                RowKind::Heading { label, trailing } => {
                    draw(canvas, label, theme.text_tertiary);
                    draw(canvas, trailing, theme.text_tertiary);
                }
                RowKind::Conversation {
                    index,
                    avatar,
                    initial,
                    name,
                    preview,
                    badge,
                } => {
                    let bg = match (hot, self.keyboard) {
                        (true, true) => theme.brand_soft,
                        (true, false) => theme.muted,
                        _ => theme.muted.alpha(0.6),
                    };
                    canvas.fill_round(row.rect, theme.radius_md, bg);
                    canvas.ring(row.rect, theme.radius_md, theme.border_subtle.alpha(0.5));
                    let conv = self.snapshot.conversations.get(*index);
                    let image = conv
                        .and_then(ConvItem::avatar_key)
                        .and_then(|k| self.avatars.image(&k));
                    match image {
                        Some(image) => canvas.image_round(&image, *avatar, theme.radius_md),
                        None => {
                            canvas.fill_round(*avatar, theme.radius_md, theme.brand_soft);
                            if conv.is_some_and(|c| c.is_group) {
                                canvas.icon(
                                    Icon::Users,
                                    avatar.x + 5.5,
                                    avatar.y + 5.5,
                                    19.0,
                                    1.65,
                                    theme.brand,
                                    false,
                                );
                            } else {
                                draw(canvas, initial, theme.brand);
                            }
                        }
                    }
                    canvas.ring(*avatar, theme.radius_md, theme.border_subtle);
                    draw(canvas, name, theme.text);
                    draw(canvas, preview, theme.text_tertiary);
                    badge.paint(canvas, theme);
                }
                RowKind::Empty { label } => {
                    canvas.icon(
                        Icon::Bell,
                        (WIDTH - 23.0) / 2.0,
                        row.rect.y + 20.25,
                        23.0,
                        1.5,
                        theme.text_tertiary,
                        false,
                    );
                    draw(canvas, label, theme.text_tertiary);
                }
                RowKind::Error { text } => {
                    canvas.fill_round(row.rect, theme.radius_md, theme.danger_bg);
                    draw(canvas, text, theme.danger);
                    canvas.icon(
                        Icon::X,
                        row.rect.right() - 8.0 - 15.0,
                        row.rect.y + 6.0,
                        13.0,
                        2.0,
                        theme.danger,
                        false,
                    );
                }
            }
        }
    }
}

fn push_separator(rows: &mut Vec<Row>, width: f32, y: f32) -> f32 {
    rows.push(Row {
        rect: Rect::new(8.0, y + 2.0, width - 16.0, 1.0),
        kind: RowKind::Separator,
        action: None,
    });
    y + SEPARATOR_H
}

fn menu_row(row: ActionRow, trailing: Trailing, action: Action) -> Row {
    Row {
        rect: row.rect,
        kind: RowKind::Menu { row, trailing },
        action: Some(action),
    }
}

/// .chat-tray-conversation：padding 6px 8px，gap 8；头像 30 垂直居中，文字列 15 + 2 + 12.6 居中。
fn conversation_kind(index: usize, conv: &ConvItem, rect: Rect) -> RowKind {
    let badge = Badge::new(conv.unread, rect.right() - 8.0, rect.y + rect.h / 2.0);
    let text_x = rect.x + 46.0;
    let text_w = badge.rect.x - 8.0 - text_x;
    let avatar = Rect::new(rect.x + 8.0, rect.y + 7.0, 30.0, 30.0);
    let preview = if conv.preview.trim().is_empty() {
        "新消息"
    } else {
        conv.preview.as_str()
    };
    RowKind::Conversation {
        index,
        avatar,
        initial: centered(
            &initial(&conv.name),
            TextStyle::sans(11.0, 600, 11.0),
            avatar,
        ),
        name: text(
            &conv.name,
            TextStyle::sans(12.0, 500, 15.0),
            text_w,
            text_x,
            rect.y + 7.2,
        ),
        preview: text(
            preview,
            TextStyle::sans(10.5, 400, 12.6),
            text_w,
            text_x,
            rect.y + 24.2,
        ),
        badge,
    }
}

// ---------- 业务外壳 ----------

pub struct ChatTrayPanelNative {
    app: tauri::AppHandle,
    bot_id: RefCell<String>,
    qq: RefCell<String>,
    anchor: std::cell::Cell<(i32, i32)>,
    view: RefCell<ChatTrayView>,
    panel: RefCell<Option<Rc<NativePanel>>>,
    // 每次 show 加一；快照回来时代次不对说明期间又开过别的（悬停后紧跟右键），丢掉
    generation: std::cell::Cell<u32>,
    pending_hover: RefCell<Option<PendingHover>>,
    opening: std::cell::Cell<Option<Mode>>,
}

struct PendingHover {
    bot: String,
    qq: String,
    anchor: (i32, i32),
    generation: u32,
}

thread_local! {
    static CONTENT: RefCell<Option<Rc<ChatTrayPanelNative>>> = const { RefCell::new(None) };
}

fn content() -> Option<Rc<ChatTrayPanelNative>> {
    CONTENT.with(|slot| slot.borrow().clone())
}

fn with_content(f: impl FnOnce(&Rc<ChatTrayPanelNative>)) {
    if let Some(content) = content() {
        f(&content);
    }
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

impl ChatTrayPanelNative {
    fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            bot_id: RefCell::new(String::new()),
            qq: RefCell::new(String::new()),
            anchor: std::cell::Cell::new((0, 0)),
            view: RefCell::new(ChatTrayView::new(Mode::Preview)),
            panel: RefCell::new(None),
            generation: std::cell::Cell::new(0),
            pending_hover: RefCell::new(None),
            opening: std::cell::Cell::new(None),
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
    fn with_view<R>(&self, f: impl FnOnce(&mut ChatTrayView) -> R) -> Option<R> {
        self.view.try_borrow_mut().ok().map(|mut v| f(&mut v))
    }

    fn ids(&self) -> (String, String) {
        let bot = self
            .bot_id
            .try_borrow()
            .map(|b| b.clone())
            .unwrap_or_default();
        let qq = self.qq.try_borrow().map(|q| q.clone()).unwrap_or_default();
        (bot, qq)
    }

    fn next_generation(&self) -> u32 {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        generation
    }

    fn cancel_pending_hover(&self) {
        if self.pending_hover.borrow_mut().take().is_some() {
            self.next_generation();
        }
    }

    fn request_hover(self: &Rc<Self>, bot: String, qq: String, anchor: (i32, i32)) {
        let panel = self.panel();
        let mode = self
            .view
            .try_borrow()
            .map(|v| v.mode)
            .unwrap_or(Mode::Preview);
        if mode == Mode::Menu && panel.as_ref().is_some_and(|p| p.is_shown()) {
            return;
        }
        if self.ids() == (bot.clone(), qq.clone())
            && let Some(panel) = panel.filter(|p| p.is_shown())
        {
            self.cancel_pending_hover();
            if !panel.is_visible() {
                panel.show_with(
                    self.anchor.get(),
                    false,
                    crate::native_panel::window::hover_panel_xy,
                );
            }
            return;
        }
        if self.ids() == (bot.clone(), qq.clone()) && self.opening.get().is_some() {
            return;
        }
        if self
            .pending_hover
            .borrow()
            .as_ref()
            .is_some_and(|p| p.bot == bot && p.qq == qq)
        {
            return;
        }
        let generation = self.next_generation();
        *self.pending_hover.borrow_mut() = Some(PendingHover {
            bot,
            qq,
            anchor,
            generation,
        });
        crate::tray_panel_native::schedule_release(&self.app);
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|c| {
                    if c.generation.get() != generation {
                        return;
                    }
                    let pending = c.pending_hover.borrow_mut().take();
                    if let Some(pending) = pending.filter(|p| cursor_on_icon(p.anchor)) {
                        c.show(pending.bot, pending.qq, pending.anchor, Mode::Preview);
                    }
                });
            });
        });
    }

    fn mark_dirty(&self) {
        self.with_view(|v| v.layout = None);
        if let Some(panel) = self.panel() {
            panel.resize_to_content();
        }
    }

    fn apply_snapshot(&self, snapshot: Snapshot) {
        let keys = self
            .with_view(|v| {
                v.set_snapshot(snapshot);
                v.claim_avatars()
            })
            .unwrap_or_default();
        tray_avatars::spawn_loads(&self.app, keys, deliver_avatar);
        self.mark_dirty();
    }

    /// 打开面板：menu 模式拿焦点，preview 模式悬停。数据回来再显示。只从主线程调。
    fn show(self: &Rc<Self>, bot: String, qq: String, anchor: (i32, i32), mode: Mode) {
        // 菜单开着时光标划过托盘图标会再来一次 Enter，不能把菜单换成预览
        let menu_open = self.panel().is_some_and(|p| p.is_visible())
            && self.view.try_borrow().is_ok_and(|v| v.mode == Mode::Menu);
        if mode == Mode::Preview && menu_open {
            return;
        }
        let panel = self.panel();
        let same_account = self.ids() == (bot.clone(), qq.clone());
        if mode == Mode::Menu && same_account {
            if menu_open {
                if let Some(panel) = panel {
                    panel.focus();
                }
                return;
            }
            if self.opening.get() == Some(Mode::Menu) {
                return;
            }
        }
        self.cancel_pending_hover();
        let shown = panel.as_ref().is_some_and(|p| p.is_shown());
        if !same_account
            && shown
            && let Some(panel) = panel.as_ref()
        {
            panel.hide();
        }
        let anchor = if same_account && shown {
            self.anchor.get()
        } else {
            anchor
        };
        let generation = self.next_generation();
        self.opening.set(Some(mode));
        if let Ok(mut slot) = self.bot_id.try_borrow_mut() {
            slot.clone_from(&bot);
        }
        if let Ok(mut slot) = self.qq.try_borrow_mut() {
            slot.clone_from(&qq);
        }
        self.anchor.set(anchor);
        let reuse = same_account && shown;
        if !reuse {
            crate::tray_panel_native::sync_ui_preferences(&self.app);
        }
        let animate_mode = same_account
            && shown
            && self.view.try_borrow().is_ok_and(|v| v.mode != mode)
            && panel.as_ref().is_some_and(|panel| {
                panel.crossfade_content(crate::native_panel::window::transition_duration())
            });
        self.with_view(|v| {
            if !same_account || !shown {
                v.mode_transition = None;
                v.layout = None;
                v.hovered = None;
                v.keyboard = false;
            }
            v.switch_mode(mode, animate_mode);
        });
        if reuse && let Some(panel) = panel {
            panel.show_with(
                anchor,
                mode == Mode::Menu,
                crate::native_panel::window::hover_panel_xy,
            );
            self.opening.set(None);
            return;
        }
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = load_snapshot(&app, &bot, &qq).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|c| {
                    if c.generation.get() != generation {
                        return;
                    }
                    c.opening.set(None);
                    if snapshot.self_id != qq {
                        return;
                    }
                    if mode == Mode::Preview && !reuse && !cursor_on_icon(anchor) {
                        return;
                    }
                    c.apply_snapshot(snapshot);
                    if !reuse && let Some(panel) = c.panel() {
                        panel.refresh_theme();
                        panel.show_with(
                            anchor,
                            mode == Mode::Menu,
                            crate::native_panel::window::hover_panel_xy,
                        );
                    }
                });
            });
        });
    }

    /// Bot / chat 事件到达时，面板开着就重取一轮。只从主线程调。
    fn refresh_if_visible(&self) {
        if !self.panel().is_some_and(|p| p.is_visible()) {
            return;
        }
        let (bot, qq) = self.ids();
        let generation = self.generation.get();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = load_snapshot(&app, &bot, &qq).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|c| {
                    if c.generation.get() == generation && c.panel().is_some_and(|p| p.is_visible())
                    {
                        c.apply_snapshot(snapshot);
                    }
                });
            });
        });
    }

    fn do_action(self: &Rc<Self>, action: Action) {
        let app = self.app.clone();
        let (bot, qq) = self.ids();
        match action {
            Action::Open => {
                tauri::async_runtime::spawn(async move {
                    let _ = crate::chat_window::open_from_tray(app, bot, qq, None).await;
                });
            }
            Action::OpenConv(index) => {
                let key = self
                    .view
                    .try_borrow()
                    .ok()
                    .and_then(|v| v.snapshot.conversations.get(index).map(|c| c.key.clone()));
                let Some(key) = key else { return };
                tauri::async_runtime::spawn(async move {
                    let _ = crate::chat_window::open_from_tray(app, bot, qq, Some(key)).await;
                });
            }
            Action::Console => {
                tauri::async_runtime::spawn(async move {
                    let _ = crate::commands::tray::window_show(app).await;
                });
            }
            Action::Background => {
                let generation = self.generation.get();
                let background_now = self
                    .view
                    .try_borrow()
                    .map(|v| v.snapshot.background)
                    .unwrap_or(false);
                tauri::async_runtime::spawn(async move {
                    let Some(state) = app.try_state::<crate::AppState>() else {
                        return;
                    };
                    let result = state
                        .chat
                        .update_tray_preference(&bot, &qq, Some(!background_now), None)
                        .await;
                    let snapshot = load_snapshot(&app, &bot, &qq).await;
                    let _ = app.run_on_main_thread(move || {
                        with_content(|c| {
                            if c.generation.get() != generation {
                                return;
                            }
                            c.apply_snapshot(snapshot);
                            if let Err(e) = result {
                                c.with_view(|v| v.error = Some(e.to_string()));
                                c.mark_dirty();
                            }
                        });
                    });
                });
            }
            Action::HideTray => {
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = app.try_state::<crate::AppState>() {
                        let _ = state
                            .chat
                            .update_tray_preference(&bot, &qq, None, Some(false))
                            .await;
                    }
                });
            }
            Action::DismissError => {
                self.with_view(|v| v.error = None);
                self.mark_dirty();
            }
        }
    }
}

pub struct ChatTrayPanelCell(pub Rc<ChatTrayPanelNative>);

impl PanelContent for ChatTrayPanelCell {
    fn finish_transition(&mut self) {
        self.0.with_view(|view| view.mode_transition = None);
    }

    fn is_tray_menu(&self) -> bool {
        self.0.view.try_borrow().is_ok_and(|v| v.mode == Mode::Menu)
    }

    fn measure(&mut self, width: f32, _theme: &Theme) -> (f32, f32) {
        let width = if width <= 0.0 { WIDTH } else { width };
        let height = self
            .0
            .with_view(|v| {
                if v.layout.is_none() {
                    v.layout(width);
                }
                v.frame_height()
            })
            .unwrap_or(MIN_HEIGHT);
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
            .with_view(|v| v.hit(x, y).and_then(|i| v.action_at(i)))
            .flatten();
        let Some(action) = action else { return false };
        self.0.do_action(action);
        // 切后台接收、关错误条时面板留着，其它点完收起
        !matches!(action, Action::Background | Action::DismissError)
    }

    fn hover(&mut self, x: f32, y: f32, inside: bool, _theme: &Theme) {
        self.0.with_view(|v| v.hover(x, y, inside));
    }

    fn update_hover(&mut self, x: f32, y: f32, inside: bool, _theme: &Theme) -> bool {
        self.0
            .with_view(|v| {
                let before = (v.hovered, v.keyboard);
                v.hover(x, y, inside);
                before != (v.hovered, v.keyboard)
            })
            .unwrap_or(false)
    }

    fn key(&mut self, vk: u16, _theme: &Theme) -> bool {
        if self.0.view.try_borrow().is_ok_and(|v| v.mode_changing()) {
            return false;
        }
        if vk == 0x0D {
            // 先取出动作再放掉视图借用：do_action 会经 mark_dirty 再借一次
            let action = self
                .0
                .with_view(|v| v.hovered.and_then(|i| v.action_at(i)))
                .flatten();
            let Some(action) = action else { return false };
            self.0.do_action(action);
            if !matches!(action, Action::Background | Action::DismissError)
                && let Some(panel) = self.0.panel()
            {
                panel.hide();
            }
            return true;
        }
        self.0.with_view(|v| v.key(vk)).unwrap_or(false)
    }

    fn animate(&mut self, _dt_ms: u32, _theme: &Theme) -> bool {
        false
    }

    fn is_hover_panel(&self) -> bool {
        self.0
            .view
            .try_borrow()
            .map(|v| v.mode == Mode::Preview)
            .unwrap_or(false)
    }

    fn hover_anchor(&self) -> Option<(i32, i32, i32)> {
        self.is_hover_panel().then(|| {
            let anchor = self.0.anchor.get();
            (anchor.0, anchor.1, 24)
        })
    }

    fn dismiss(&mut self) {
        let ids = self.0.ids();
        let next_account = self
            .0
            .pending_hover
            .borrow()
            .as_ref()
            .is_some_and(|p| p.bot != ids.0 || p.qq != ids.1);
        if !next_account {
            self.0.cancel_pending_hover();
            self.0.next_generation();
        }
        self.0.opening.set(None);
    }
}

// ---------- 后台数据 ----------

async fn load_snapshot(app: &tauri::AppHandle, bot_id: &str, qq: &str) -> Snapshot {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return Snapshot::default();
    };
    let Ok(tray) = state.chat.tray_snapshot(bot_id, qq).await else {
        return Snapshot::default();
    };
    let account = tray.account;
    let status = connection_text_of(&account);
    let online = status == "在线";
    let conversations = tray
        .conversations
        .iter()
        .map(|c| ConvItem {
            key: c.key.clone(),
            name: c.name.clone(),
            preview: c.preview.clone(),
            unread: c.unread,
            id: c.id.clone(),
            is_group: matches!(
                c.kind,
                ncd_domain::chat_archive::ChatArchiveConversationKind::Group
            ),
        })
        .collect();
    Snapshot {
        target_name: account.target.name.clone(),
        self_id: account.preference.self_id.clone(),
        status,
        online,
        background: account.preference.background,
        notification_unread: account.notification_unread,
        conversations,
        conversation_count: tray.conversation_count,
    }
}

fn connection_text_of(account: &ncd_domain::chat_desktop::ChatAccountStatus) -> String {
    if !account.target.running || account.target.online == Some(false) {
        return "离线".to_string();
    }
    if !account.preference.background {
        return "按需接收".to_string();
    }
    match &account.connection {
        ncd_domain::onebot_debug::DebugReceiverState::Connected => "在线".into(),
        ncd_domain::onebot_debug::DebugReceiverState::Connecting => "连接中".into(),
        ncd_domain::onebot_debug::DebugReceiverState::Reconnecting { .. } => "重新连接中".into(),
        ncd_domain::onebot_debug::DebugReceiverState::Stopped { .. } => "接收已暂停".into(),
    }
}

// ---------- 对外入口 ----------

fn cursor_on_icon(anchor: (i32, i32)) -> bool {
    let scale = crate::windows_ui::monitor_dpi_scale_at(anchor.0, anchor.1).max(0.25);
    let radius = (20.0 * scale).ceil() as i32;
    crate::native_panel::sys::cursor_pos()
        .is_some_and(|(x, y)| (x - anchor.0).abs() <= radius && (y - anchor.1).abs() <= radius)
}

fn ensure_content(app: tauri::AppHandle, anchor: (i32, i32)) -> Option<Rc<ChatTrayPanelNative>> {
    if let Some(content) = content() {
        return Some(content);
    }
    crate::tray_panel_native::sync_ui_preferences(&app);
    let content = Rc::new(ChatTrayPanelNative::new(app));
    let cell = Box::new(ChatTrayPanelCell(content.clone())) as Box<dyn PanelContent>;
    match NativePanel::create(anchor, WIDTH, cell) {
        Ok(panel) => {
            content.set_panel(Rc::new(panel));
            CONTENT.with(|slot| *slot.borrow_mut() = Some(content.clone()));
            Some(content)
        }
        Err(e) => {
            tracing::warn!("native chat tray create: {e}");
            None
        }
    }
}

pub fn hover(app: &tauri::AppHandle, bot: String, qq: String, anchor: (i32, i32)) {
    let app_owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(content) = ensure_content(app_owned, anchor) {
            content.request_hover(bot, qq, anchor);
        }
    });
}

pub fn cancel_hover(app: &tauri::AppHandle, bot: String, qq: String) {
    let _ = app.run_on_main_thread(move || {
        with_content(|c| {
            let pending =
                c.pending_hover.borrow().as_ref().is_some_and(|p| {
                    p.bot == bot && p.qq == qq && p.generation == c.generation.get()
                });
            if pending {
                c.cancel_pending_hover();
            }
            if c.ids() == (bot, qq)
                && c.opening.get() == Some(Mode::Preview)
                && !c.panel().is_some_and(|p| p.is_shown())
            {
                c.next_generation();
                c.opening.set(None);
            }
        });
    });
}

/// 右键菜单入口（menu=true）；悬停预览入口（menu=false）。
pub fn show(app: &tauri::AppHandle, bot: String, qq: String, anchor: (i32, i32), menu: bool) {
    if !menu {
        hover(app, bot, qq, anchor);
        return;
    }
    crate::tray_panel_native::schedule_release(app);
    let app_owned = app.clone();
    let mode = if menu { Mode::Menu } else { Mode::Preview };
    let _ = app.run_on_main_thread(move || {
        if let Some(content) = ensure_content(app_owned, anchor) {
            content.show(bot, qq, anchor, mode);
        }
    });
}

/// 收着的账号托盘面板连窗口一起销毁，见 tray_panel_native::schedule_release。只从主线程调。
pub(crate) fn release_if_hidden() -> bool {
    let Some(content) = content() else {
        return true;
    };
    if content.panel().is_some_and(|p| p.is_shown())
        || content.pending_hover.borrow().is_some()
        || content.opening.get().is_some()
    {
        return false;
    }
    if let Some(panel) = content.panel() {
        panel.destroy();
    }
    CONTENT.with(|slot| slot.borrow_mut().take());
    true
}

/// 收起面板。
pub fn hide(app: &tauri::AppHandle) {
    let _ = app.run_on_main_thread(move || {
        with_content(|c| {
            c.cancel_pending_hover();
            c.next_generation();
            c.opening.set(None);
            if let Some(panel) = c.panel() {
                panel.hide();
            }
        });
    });
}

/// Bot 事件 / 消息到达时刷一轮开着的面板。
pub fn refresh_if_visible(app: &tauri::AppHandle) {
    let _ = app.run_on_main_thread(move || {
        with_content(|c| c.refresh_if_visible());
    });
}

#[cfg(all(test, windows))]
pub(crate) mod tests {
    use super::*;

    pub(crate) struct MotionShot(pub ChatTrayView);

    impl PanelContent for MotionShot {
        fn measure(&mut self, width: f32, _: &Theme) -> (f32, f32) {
            (WIDTH, self.0.layout(width))
        }
        fn paint(&mut self, canvas: &Canvas, _: f32, _: f32, _: f32, theme: &Theme) {
            self.0.paint(canvas, theme);
        }
        fn click(&mut self, _: f32, _: f32, _: &Theme) -> bool {
            false
        }
        fn hover(&mut self, x: f32, y: f32, inside: bool, _: &Theme) {
            self.0.hover(x, y, inside);
        }
        fn key(&mut self, _: u16, _: &Theme) -> bool {
            false
        }
        fn animate(&mut self, _: u32, _: &Theme) -> bool {
            false
        }
    }

    #[test]
    fn mode_transition_progress_finishes_without_a_timer_tick_count() {
        let start = Instant::now();
        let transition = ModeTransition {
            start,
            duration_ms: 180,
        };
        assert_eq!(transition.progress(start), 0.0);
        assert_eq!(transition.progress(start + Duration::from_millis(180)), 1.0);
        assert_eq!(transition.progress(start + Duration::from_millis(500)), 1.0);
    }

    #[test]
    fn mode_transition_keeps_final_size_and_blocks_actions_while_the_body_crossfades() {
        let mut view = ChatTrayView::new(Mode::Menu);
        view.layout = Some(Layout {
            rows: vec![Row {
                rect: Rect::new(0.0, 0.0, WIDTH, 100.0),
                kind: RowKind::Empty { label: None },
                action: Some(Action::Open),
            }],
            height: 172.5,
        });
        view.mode_transition = Some(ModeTransition {
            start: Instant::now(),
            duration_ms: 180,
        });
        assert_eq!(
            view.frame_height(),
            172.5,
            "crossfade must not resize each frame"
        );
        assert_eq!(view.hit(40.0, 50.0), None);
        assert!(!view.key(0x28));
        view.mode_transition = None;
        assert_eq!(view.hit(40.0, 50.0), Some(0));
    }

    pub(crate) fn conv(i: usize, group: bool, unread: u32) -> ConvItem {
        ConvItem {
            key: format!("k{i}"),
            name: if group {
                format!("开发交流群 {i}")
            } else {
                format!("好友 {i}")
            },
            preview: if i == 0 {
                String::new()
            } else {
                "晚上一起吃饭吗？明天的会议改到下午三点了".into()
            },
            unread,
            id: format!("{}", 20000 + i),
            is_group: group,
        }
    }

    pub(crate) fn view_with(mode: Mode, convs: Vec<ConvItem>, count: usize) -> ChatTrayView {
        let mut v = ChatTrayView::new(mode);
        v.set_snapshot(Snapshot {
            target_name: "AQiaoYO".into(),
            self_id: "2707600964".into(),
            status: "在线".into(),
            online: true,
            background: true,
            notification_unread: 3,
            conversations: convs,
            conversation_count: count,
        });
        v
    }

    #[test]
    fn preview_and_menu_share_height_for_every_conversation_count() {
        for count in 0..=8 {
            let conversations: Vec<_> = (0..count).map(|i| conv(i, i == 1, 2)).collect();
            let mut preview = view_with(Mode::Preview, conversations.clone(), count);
            let mut menu = view_with(Mode::Menu, conversations, count);
            let expected = (104.0 + 48.0 * count.min(MAX_ROWS) as f32).max(MIN_HEIGHT);
            for view in [&mut preview, &mut menu] {
                let height = view.layout(WIDTH);
                assert_eq!(height, expected, "count={count}");
                for row in &view.layout.as_ref().unwrap().rows {
                    assert!(row.rect.y >= 0.0 && row.rect.bottom() <= height);
                }
            }
            preview.switch_mode(Mode::Menu, true);
            assert!((preview.frame_height() - expected).abs() < 0.01);
            assert_eq!(preview.layout(WIDTH), expected);
            preview.mode_transition = None;
            preview.switch_mode(Mode::Preview, false);
            assert_eq!(preview.layout(WIDTH), expected);
        }
    }

    #[test]
    fn menu_uses_shared_space_without_stretching_actions_or_badges() {
        let mut menu = view_with(Mode::Menu, (0..5).map(|i| conv(i, false, 1)).collect(), 5);
        menu.layout(WIDTH);
        for row in &menu.layout.as_ref().unwrap().rows {
            if let RowKind::Menu {
                row: action,
                trailing,
            } = &row.kind
            {
                assert!((action.rect.h - 44.0).abs() < 0.01);
                assert_eq!(row.rect, action.rect);
                if let Trailing::Badge(badge) = trailing {
                    assert!(
                        (badge.rect.y + badge.rect.h / 2.0 - action.rect.y - action.rect.h / 2.0)
                            .abs()
                            < 0.01
                    );
                }
            }
        }
    }

    #[test]
    fn error_strip_keeps_preview_and_menu_the_same_height() {
        let mut preview = view_with(Mode::Preview, vec![], 0);
        let mut menu = view_with(Mode::Menu, vec![], 0);
        preview.error = Some("操作失败".into());
        menu.error = preview.error.clone();
        assert_eq!(preview.layout(WIDTH), menu.layout(WIDTH));
    }

    #[test]
    fn more_conversations_than_shown_offer_view_all() {
        let six: Vec<_> = (0..6).map(|i| conv(i, false, 1)).collect();
        let mut v = view_with(Mode::Preview, six, 6);
        v.layout(WIDTH);
        let last = v.layout.as_ref().unwrap().rows.last().unwrap();
        let RowKind::Menu { row, .. } = &last.kind else {
            panic!("last row should be the action row");
        };
        assert!(row.label.is_some());
        assert_eq!(last.action, Some(Action::Open));
    }

    #[test]
    fn keyboard_cycles_actionable_rows_without_panicking() {
        let mut v = view_with(Mode::Menu, vec![], 0);
        v.layout(WIDTH);
        assert!(v.key(0x28));
        assert_eq!(v.action_at(v.hovered.unwrap()), Some(Action::Open));
        assert!(v.key(0x26));
        assert_eq!(v.action_at(v.hovered.unwrap()), Some(Action::HideTray));
        assert!(v.key(0x28));
        assert_eq!(v.action_at(v.hovered.unwrap()), Some(Action::Open));
        assert!(v.key(0x23));
        assert_eq!(v.action_at(v.hovered.unwrap()), Some(Action::HideTray));
        let mut empty = ChatTrayView::new(Mode::Preview);
        assert!(!empty.key(0x28), "no layout yet");
        empty.layout(WIDTH);
        assert!(empty.key(0x28));
    }

    #[test]
    #[ignore = "需要 NCD_PANEL_SHOTS_DIR 和交互桌面"]
    fn render_chat_panel_shots() {
        use crate::native_panel::theme::{UiPreferences, set_ui_preferences, theme};
        if crate::native_panel::shots::dir().is_none() {
            return;
        }
        set_ui_preferences(UiPreferences {
            theme: "latte".into(),
            radius_style: "square".into(),
        });
        let fill = |mut v: ChatTrayView| {
            let px = crate::tray_panel_native::tests::fake_avatar;
            if let Some(k) = AvatarKey::user("2707600964") {
                v.avatars.fill(k, Some(px(3)));
            }
            if let Some(k) = AvatarKey::user("20000") {
                v.avatars.fill(k, Some(px(5)));
            }
            v
        };
        type Case = (&'static str, Box<dyn Fn() -> ChatTrayView>);
        let cases: Vec<Case> = vec![
            (
                "native-chat-menu",
                Box::new(move || {
                    let mut v = fill(view_with(Mode::Menu, vec![], 0));
                    v.layout(WIDTH);
                    v.hover(100.0, 44.5 + 2.0 + 26.0 + 10.0, true);
                    v
                }),
            ),
            (
                "native-chat-preview",
                Box::new(move || {
                    let mut v = fill(view_with(
                        Mode::Preview,
                        vec![conv(0, false, 2), conv(1, true, 120), conv(2, false, 1)],
                        7,
                    ));
                    v.layout(WIDTH);
                    v.hover(100.0, 44.5 + 22.5 + 2.0 + 48.0 + 20.0, true);
                    v
                }),
            ),
            (
                "native-chat-empty",
                Box::new(move || fill(view_with(Mode::Preview, vec![], 0))),
            ),
        ];
        for (name, make) in &cases {
            for (scale, pct) in [(1.0, 100), (1.25, 125), (1.5, 150)] {
                let mut v = make();
                let h = v.layout(WIDTH);
                crate::native_panel::shots::offscreen(&format!("{name}-{pct}"), scale, h, |c| {
                    v.paint(c, &theme());
                });
            }
        }
    }

    #[test]
    fn conversation_text_clears_badge() {
        let mut v = view_with(Mode::Preview, vec![conv(1, true, 120)], 1);
        v.layout(WIDTH);
        for row in &v.layout.as_ref().unwrap().rows {
            if let RowKind::Conversation {
                name,
                preview,
                badge,
                ..
            } = &row.kind
            {
                for t in [name, preview] {
                    assert!(t.as_ref().unwrap().right() <= badge.rect.x - 8.0 + 0.01);
                }
                assert!(badge.rect.w > 14.0, "99+ is wider than the minimum pill");
            }
        }
    }
}
