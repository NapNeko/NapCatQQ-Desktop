// 账号托盘面板的原生实现：右键弹菜单，悬停弹未读预览。
//
// 和主托盘共用 native_panel 底座；布局对齐 `src-ui/modules/tray/ChatTrayPanel.tsx`。
// 两种形态（menu=true / false）共用同一个窗口，打开时换 content.mode，不重开 hwnd。
// 线程模型和主托盘一样：单例只在主线程，所有外部调用先 run_on_main_thread 切过去。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tauri::Manager;

use crate::native_panel::gfx::{Canvas, Image, Rect, TextBox, TextStyle, shared};
use crate::native_panel::icons::Icon;
use crate::native_panel::theme::{Rgba, Theme};
use crate::native_panel::window::{NativePanel, PanelContent};

const WIDTH: f32 = 260.0;

#[derive(Clone)]
pub struct ConvItem {
    pub key: String,
    pub name: String,
    pub preview: String,
    pub unread: u32,
    pub id: String,
    pub is_group: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Open,
    Console,
    Background,
    HideTray,
    OpenConv(usize),
    DismissError,
}

enum RowKind {
    Header,
    Separator,
    Menu { icon: Icon, label: String, checked: Option<bool> },
    Heading { label: String, trailing: String },
    Conversation(usize),
    Empty { text: String },
    ActionRow { icon: Icon, label: String },
    Error { text: String },
}

struct Row {
    rect: Rect,
    action: Option<Action>,
    kind: RowKind,
    hover: Rgba,
}

struct Layout {
    rows: Vec<Row>,
    height: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Menu,
    Preview,
}

pub struct ChatTrayPanelNative {
    app: tauri::AppHandle,
    bot_id: RefCell<String>,
    qq: RefCell<String>,
    mode: Cell<Mode>,
    snapshot: RefCell<Snapshot>,
    error: RefCell<Option<String>>,
    busy: Cell<bool>,
    layout: RefCell<Option<Rc<Layout>>>,
    texts: RefCell<HashMap<usize, TextBox>>,
    hovered: Cell<Option<usize>>,
    scroll: Cell<f32>,
    avatars: RefCell<HashMap<String, Rc<Image>>>,
    anchor_last: std::cell::Cell<(i32, i32)>,
    panel: RefCell<Option<Rc<NativePanel>>>,
}

thread_local! {
    static CONTENT: RefCell<Option<Rc<ChatTrayPanelNative>>> = const { RefCell::new(None) };
}

fn content() -> Option<Rc<ChatTrayPanelNative>> {
    CONTENT.with(|slot| slot.borrow().clone())
}

fn with_content(f: impl FnOnce(&Rc<ChatTrayPanelNative>)) {
    CONTENT.with(|slot| {
        if let Some(content) = slot.borrow().as_ref() {
            f(content);
        }
    });
}

impl ChatTrayPanelNative {
    fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            bot_id: RefCell::new(String::new()),
            qq: RefCell::new(String::new()),
            mode: Cell::new(Mode::Preview),
            snapshot: RefCell::new(Snapshot::default()),
            error: RefCell::new(None),
            busy: Cell::new(false),
            layout: RefCell::new(None),
            texts: RefCell::new(HashMap::new()),
            hovered: Cell::new(None),
            scroll: Cell::new(0.0),
            avatars: RefCell::new(HashMap::new()),
            anchor_last: std::cell::Cell::new((0, 0)),
            panel: RefCell::new(None),
        }
    }

    fn set_panel(&self, panel: Rc<NativePanel>) {
        *self.panel.borrow_mut() = Some(panel);
    }

    fn panel(&self) -> Option<Rc<NativePanel>> {
        self.panel.borrow().clone()
    }

    fn mark_dirty(&self) {
        self.layout.borrow_mut().take();
        self.texts.borrow_mut().clear();
        if let Some(panel) = self.panel() {
            panel.resize_to_content();
        }
    }

    fn apply_snapshot(&self, snapshot: Snapshot) {
        *self.snapshot.borrow_mut() = snapshot;
        self.error.borrow_mut().take();
        self.mark_dirty();
    }

    /// 打开面板：menu 模式拿焦点，preview 模式悬停。
    /// 只从主线程调。
    fn show(self: &Rc<Self>, bot: String, qq: String, anchor: (i32, i32), mode: Mode) {
        *self.bot_id.borrow_mut() = bot.clone();
        *self.qq.borrow_mut() = qq.clone();
        self.mode.set(mode);
        self.scroll.set(0.0);
        self.hovered.set(None);
        self.anchor_last.set(anchor);
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = load_snapshot(&app, &bot, &qq).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|c| {
                    c.apply_snapshot(snapshot);
                    if let Some(panel) = c.panel() {
                        panel.resize_to_content();
                        match mode {
                            Mode::Menu => panel.show_with(anchor, true, crate::native_panel::window::tray_panel_xy),
                            Mode::Preview => panel.show_with(anchor, false, crate::native_panel::window::hover_panel_xy),
                        }
                    }
                });
            });
        });
    }

    /// Bot / chat 事件到达时，面板开着就重取一轮。只从主线程调。
    fn refresh_if_visible(&self) {
        let Some(panel) = self.panel() else { return };
        if !panel.is_visible() {
            return;
        }
        let bot = self.bot_id.borrow().clone();
        let qq = self.qq.borrow().clone();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = load_snapshot(&app, &bot, &qq).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|c| c.apply_snapshot(snapshot));
            });
        });
    }

    fn connection_text(snapshot: &Snapshot) -> String {
        snapshot.status.clone()
    }

    fn compute_layout(&self, width: f32, theme: &Theme) -> Rc<Layout> {
        let snapshot = self.snapshot.borrow();
        let mut rows: Vec<Row> = Vec::new();
        let mut y: f32 = 0.0;

        // 头部：10.75..39.5（自头像 + 名字 + 状态 + QQ）
        rows.push(Row { rect: Rect::new(0.0, 0.0, width, 39.5), action: None, kind: RowKind::Header, hover: Rgba::TRANSPARENT });
        y = 39.5;

        // 分隔线
        rows.push(Row { rect: Rect::new(8.0, y, width - 16.0, 1.0), action: None, kind: RowKind::Separator, hover: Rgba::TRANSPARENT });
        y += 1.0;

        match self.mode.get() {
            Mode::Menu => {
                y += 4.0;
                let unread = snapshot.notification_unread;
                let unread_trailing = if unread > 0 { Some(if unread > 99 { "99+".into() } else { unread.to_string() }) } else { None };
                rows.push(Row {
                    rect: Rect::new(4.0, y, width - 8.0, 26.0),
                    action: Some(Action::Open),
                    kind: RowKind::Menu { icon: Icon::MessageCircle, label: "打开聊天".into(), checked: None },
                    hover: theme.brand_soft,
                });
                if unread_trailing.is_some() {
                    // 尾部 badge 在 paint 里画
                }
                y += 28.0;
                rows.push(Row {
                    rect: Rect::new(4.0, y, width - 8.0, 26.0),
                    action: Some(Action::Console),
                    kind: RowKind::Menu { icon: Icon::Monitor, label: "打开控制台".into(), checked: None },
                    hover: theme.brand_soft,
                });
                y += 28.0;
                y = separator(&mut rows, y, width);
                y += 4.0;
                rows.push(Row {
                    rect: Rect::new(4.0, y, width - 8.0, 26.0),
                    action: Some(Action::Background),
                    kind: RowKind::Menu { icon: Icon::Radio, label: "后台接收消息".into(), checked: Some(snapshot.background) },
                    hover: theme.brand_soft,
                });
                y += 28.0;
                y = separator(&mut rows, y, width);
                y += 4.0;
                rows.push(Row {
                    rect: Rect::new(4.0, y, width - 8.0, 26.0),
                    action: Some(Action::HideTray),
                    kind: RowKind::Menu { icon: Icon::EyeOff, label: "隐藏此账号托盘".into(), checked: None },
                    hover: theme.brand_soft,
                });
                y += 28.0;
                y += 4.0;
            }
            Mode::Preview => {
                y += 4.0;
                let count = snapshot.conversation_count;
                let trailing = if count > 0 { format!("{count} 个会话") } else { String::new() };
                rows.push(Row {
                    rect: Rect::new(0.0, y, width, 18.0),
                    action: None,
                    kind: RowKind::Heading { label: "未读消息".into(), trailing },
                    hover: Rgba::TRANSPARENT,
                });
                y += 18.0;
                if snapshot.conversations.is_empty() {
                    rows.push(Row {
                        rect: Rect::new(8.0, y, width - 16.0, 88.0),
                        action: None,
                        kind: RowKind::Empty { text: "暂无新消息".into() },
                        hover: Rgba::TRANSPARENT,
                    });
                    y += 88.0;
                } else {
                    // 会话列表最多 5 条，超出滚动
                    let max_visible = 5usize;
                    let visible_count = snapshot.conversations.len().min(max_visible);
                    let list_h = visible_count as f32 * 48.0;
                    let list_top = y;
                    for (i, conv) in snapshot.conversations.iter().take(visible_count).enumerate() {
                        let rect = Rect::new(8.0, y, width - 16.0, 44.0);
                        rows.push(Row {
                            rect,
                            action: Some(Action::OpenConv(i)),
                            kind: RowKind::Conversation(i),
                            hover: theme.muted.mix(theme.text, 0.05),
                        });
                        y += 44.0 + 4.0;
                        let _ = conv;
                    }
                    // 多出来的会话走滚动；hit 区还是列表 5 条的范围，滚动偏移 paint 时扣
                    let _ = list_top;
                    let _ = list_h;
                    y -= 4.0; // 最后一条的 gap 不算
                }
                y += 4.0;
                y = separator(&mut rows, y, width);
                y += 3.0;
                rows.push(Row {
                    rect: Rect::new(4.0, y, width - 8.0, 26.0),
                    action: Some(Action::Open),
                    kind: RowKind::ActionRow { icon: Icon::MessageCircle, label: if snapshot.conversation_count > snapshot.conversations.len() { "查看全部会话".into() } else { "打开聊天".into() } },
                    hover: theme.brand_soft,
                });
                y += 28.0;
                y += 4.0;
            }
        }

        // 错误条
        if let Some(err) = self.error.borrow().clone().filter(|e| !e.is_empty()) {
            rows.push(Row {
                rect: Rect::new(8.0, y, width - 16.0, 24.0),
                action: Some(Action::DismissError),
                kind: RowKind::Error { text: err },
                hover: Rgba::TRANSPARENT,
            });
            y += 25.0;
        }

        Rc::new(Layout { rows, height: y.max(80.0) })
    }

    fn layout_text(&self, key: usize, text: &str, style: TextStyle, max_width: f32) -> Option<TextBox> {
        if let Some(t) = self.texts.borrow().get(&key) {
            return Some(clone_tb(t));
        }
        let shared = shared().ok()?;
        let t = shared.text.layout(text, style, max_width).ok()?;
        self.texts.borrow_mut().insert(key, clone_tb(&t));
        Some(t)
    }

    fn draw_text(&self, canvas: &Canvas, key: usize, text: &str, style: TextStyle, x: f32, y: f32, color: Rgba) {
        if let Some(t) = self.layout_text(key, text, style, 2_000.0) {
            canvas.text(&t, x, y, color);
        }
    }

    fn draw_text_clipped(&self, canvas: &Canvas, key: usize, text: &str, style: TextStyle, x: f32, y: f32, max_width: f32, color: Rgba) {
        if let Some(t) = self.layout_text(key, text, style, max_width) {
            canvas.text(&t, x, y, color);
        }
    }

    fn paint_avatar(&self, canvas: &Canvas, rect: Rect, is_self: bool, name: &str, id: &str, _is_group: bool, theme: &Theme) {
        // 头像：先尝试用 chat_tray 的磁盘缓存（state/chat/avatars/{qq}-128.png）
        // 读不到就画首字母占位
        let cached = if !is_self {
            let state = self.app.state::<crate::AppState>();
            let path = state.data_root.join(format!("state/chat/avatars/{id}-128.png"));
            std::fs::read(&path).ok().and_then(|bytes| decode_png(&bytes))
        } else {
            None
        };
        if let Some(img) = cached {
            canvas.image_round(&img, rect, theme.radius_sm);
        } else {
            // 首字母 + brand_soft 底
            canvas.fill_round(rect, theme.radius_sm, theme.brand_soft);
            let letter = name.chars().next().unwrap_or('?').to_ascii_uppercase().to_string();
            if let Some(t) = self.layout_text(9000 + id.len(), &letter, TextStyle::sans(11.0, 600, 11.0), rect.w) {
                canvas.text(&t, rect.x + (rect.w - t.width) / 2.0, rect.y + (rect.h - t.height) / 2.0, theme.brand);
            }
        }
    }

    fn do_action(self: &Rc<Self>, action: Action) {
        let app = self.app.clone();
        let bot = self.bot_id.borrow().clone();
        let qq = self.qq.borrow().clone();
        match action {
            Action::Open => {
                tauri::async_runtime::spawn(async move {
                    let _ = crate::chat_window::open_from_tray(app, bot, qq, None).await;
                });
            }
            Action::OpenConv(idx) => {
                let key = {
                    let snapshot = self.snapshot.borrow();
                    snapshot.conversations.get(idx).map(|c| c.key.clone())
                };
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
                let background_now = self.snapshot.borrow().background;
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<crate::AppState>();
                    let _ = state.chat.update_tray_preference(&bot, &qq, Some(!background_now), None).await;
                    let snapshot = load_snapshot(&app, &bot, &qq).await;
                    let _ = app.run_on_main_thread(move || {
                        with_content(|c| c.apply_snapshot(snapshot));
                    });
                });
            }
            Action::HideTray => {
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<crate::AppState>();
                    let _ = state.chat.update_tray_preference(&bot, &qq, None, Some(false)).await;
                    let _ = app.run_on_main_thread(move || {
                        with_content(|c| {
                            if let Some(panel) = c.panel() {
                                panel.hide();
                            }
                        });
                    });
                });
            }
            Action::DismissError => {
                self.error.borrow_mut().take();
                self.mark_dirty();
            }
        }
    }
}

fn separator(rows: &mut Vec<Row>, y: f32, width: f32) -> f32 {
    rows.push(Row { rect: Rect::new(8.0, y, width - 16.0, 1.0), action: None, kind: RowKind::Separator, hover: Rgba::TRANSPARENT });
    y + 1.0
}

fn clone_tb(t: &TextBox) -> TextBox {
    TextBox { layout: t.layout.clone(), width: t.width, height: t.height }
}

fn decode_png(bytes: &[u8]) -> Option<Rc<Image>> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(Rc::new(Image { width: w, height: h, rgba: rgba.into_raw() }))
}

// ---------- PanelContent wrapper ----------

pub struct ChatTrayPanelCell(pub Rc<ChatTrayPanelNative>);

impl PanelContent for ChatTrayPanelCell {
    fn measure(&mut self, width: f32, theme: &Theme) -> (f32, f32) {
        let this = self.0.clone();
        let layout = this.compute_layout(if width <= 0.0 { WIDTH } else { width }, theme);
        let height = layout.height;
        *this.layout.borrow_mut() = Some(layout);
        (WIDTH, height.ceil().max(80.0))
    }

    fn paint(&mut self, canvas: &Canvas, _x: f32, _y: f32, width: f32, theme: &Theme) {
        let this = self.0.clone();
        let layout_opt = this.layout.borrow().clone();
        let layout = layout_opt.unwrap_or_else(|| this.compute_layout(width, theme));
        let hovered = this.hovered.get();
        let snapshot = this.snapshot.borrow();
        let mode = this.mode.get();

        for (i, row) in layout.rows.iter().enumerate() {
            match &row.kind {
                RowKind::Header => {
                    let a = Rect::new(12.0, 10.75, 24.0, 24.0);
                    this.paint_avatar(canvas, a, true, &snapshot.target_name, &snapshot.self_id, false, theme);
                    this.draw_text(canvas, 0, &snapshot.target_name, TextStyle::sans(12.5, 600, 12.5), 42.0, 21.0, theme.text);
                    let status = ChatTrayPanelNative::connection_text(&snapshot);
                    let dot_y = 30.5;
                    canvas.circle(21.0, dot_y, 3.0, if snapshot.online { theme.success } else { theme.text_disabled });
                    this.draw_text(canvas, 1, &format!("{} · QQ: {}", status, snapshot.self_id), TextStyle::sans(11.0, 400, 11.0), 28.0, 24.5, theme.text_tertiary);
                }
                RowKind::Separator => {
                    canvas.hline(row.rect.x, row.rect.right(), row.rect.y, theme.border_subtle);
                }
                RowKind::Menu { icon, label, checked } => {
                    if hovered == Some(i) && row.hover.a > 0.0 {
                        canvas.fill_round(row.rect, theme.radius_sm, row.hover);
                    }
                    let icon_y = row.rect.y + (row.rect.h - 14.0) / 2.0;
                    canvas.icon(*icon, row.rect.x + 10.0, icon_y, 14.0, 1.9, theme.text_secondary, false);
                    let text_y = row.rect.y + (row.rect.h - 12.5) / 2.0 + 0.8;
                    this.draw_text_clipped(canvas, 200 + i, label, TextStyle::sans(12.5, 400, 12.5), row.rect.x + 34.0, text_y, row.rect.w - 70.0, theme.text);
                    if let Some(on) = checked {
                        if *on {
                            canvas.icon(Icon::Check, row.rect.right() - 28.0, icon_y, 14.0, 1.9, theme.brand, false);
                        }
                    } else if *icon == Icon::MessageCircle && snapshot.notification_unread > 0 {
                        // 未读 badge
                        let n = snapshot.notification_unread;
                        let badge_text = if n > 99 { "99+".into() } else { n.to_string() };
                        if let Some(t) = this.layout_text(300 + i, &badge_text, TextStyle::sans(10.0, 500, 14.0), 30.0) {
                            let bw = t.width.max(14.0) + 8.0;
                            let bx = row.rect.right() - 12.0 - bw;
                            let by = row.rect.y + (row.rect.h - 14.0) / 2.0;
                            canvas.fill_round(Rect::new(bx, by, bw, 14.0), 7.0, theme.danger);
                            canvas.text(&t, bx + (bw - t.width) / 2.0, by, Rgba::rgb(0xff, 0xff, 0xff));
                        }
                    }
                }
                RowKind::Heading { label, trailing } => {
                    this.draw_text(canvas, 400 + i, label, TextStyle::sans(11.0, 400, 11.0), row.rect.x + 10.0, row.rect.y + 2.0, theme.text_tertiary);
                    if !trailing.is_empty() {
                        this.draw_text(canvas, 410 + i, trailing, TextStyle::sans(11.0, 400, 11.0), row.rect.right() - 80.0, row.rect.y + 2.0, theme.text_tertiary);
                    }
                }
                RowKind::Conversation(idx) => {
                    let Some(conv) = snapshot.conversations.get(*idx) else { continue };
                    if hovered == Some(i) && row.hover.a > 0.0 {
                        canvas.fill_round(row.rect, theme.radius_sm, row.hover);
                    }
                    let a = Rect::new(row.rect.x + 6.0, row.rect.y + 6.0, 30.0, 30.0);
                    this.paint_avatar(canvas, a, false, &conv.name, &conv.id, conv.is_group, theme);
                    // 名字 + 预览
                    this.draw_text_clipped(canvas, 500 + i, &conv.name, TextStyle::sans(12.0, 500, 14.4), a.right() + 6.0, row.rect.y + 8.0, row.rect.w - 90.0, theme.text);
                    let preview = if conv.preview.is_empty() { "新消息".to_string() } else { conv.preview.clone() };
                    this.draw_text_clipped(canvas, 600 + i, &preview, TextStyle::sans(10.5, 400, 10.5), a.right() + 6.0, row.rect.y + 24.0, row.rect.w - 90.0, theme.text_tertiary);
                    // 未读 badge 右
                    let n = conv.unread;
                    if n > 0 {
                        let badge_text = if n > 99 { "99+".into() } else { n.to_string() };
                        if let Some(t) = this.layout_text(700 + i, &badge_text, TextStyle::sans(10.0, 500, 14.0), 30.0) {
                            let bw = t.width.max(14.0) + 8.0;
                            let bx = row.rect.right() - 12.0 - bw;
                            let by = row.rect.y + (row.rect.h - 14.0) / 2.0;
                            canvas.fill_round(Rect::new(bx, by, bw, 14.0), 7.0, theme.danger);
                            canvas.text(&t, bx + (bw - t.width) / 2.0, by, Rgba::rgb(0xff, 0xff, 0xff));
                        }
                    }
                }
                RowKind::Empty { text } => {
                    canvas.icon(Icon::Bell, row.rect.x + (row.rect.w - 23.0) / 2.0, row.rect.y + 16.0, 23.0, 1.5, theme.text_tertiary, false);
                    if let Some(t) = this.layout_text(800 + i, text, TextStyle::sans(11.0, 400, 11.0), row.rect.w) {
                        canvas.text(&t, row.rect.x + (row.rect.w - t.width) / 2.0, row.rect.y + 50.0, theme.text_tertiary);
                    }
                }
                RowKind::ActionRow { icon, label } => {
                    if hovered == Some(i) && row.hover.a > 0.0 {
                        canvas.fill_round(row.rect, theme.radius_sm, row.hover);
                    }
                    let icon_y = row.rect.y + (row.rect.h - 14.0) / 2.0;
                    canvas.icon(*icon, row.rect.x + 10.0, icon_y, 14.0, 1.9, theme.text_secondary, false);
                    let text_y = row.rect.y + (row.rect.h - 12.5) / 2.0 + 0.8;
                    this.draw_text_clipped(canvas, 950 + i, label, TextStyle::sans(12.5, 400, 12.5), row.rect.x + 34.0, text_y, row.rect.w - 60.0, theme.text);
                }
                RowKind::Error { text } => {
                    canvas.fill_round(row.rect, theme.radius_sm, theme.danger_bg);
                    this.draw_text_clipped(canvas, 980 + i, text, TextStyle::sans(11.0, 400, 16.0), row.rect.x + 2.0, row.rect.y + 2.0, row.rect.w - 24.0, theme.danger);
                    canvas.icon(Icon::X, row.rect.right() - 22.0, row.rect.y + 4.0, 13.0, 1.9, theme.danger, false);
                }
            }
        }
    }

    fn click(&mut self, x: f32, y: f32, _theme: &Theme) -> bool {
        let this = self.0.clone();
        let action: Option<Action> = {
            let layout = this.layout.borrow();
            layout.as_ref().and_then(|l| {
                l.rows.iter().find(|row| row.rect.contains(x, y)).and_then(|row| row.action)
            })
        };
        let Some(action) = action else { return false };
        this.do_action(action);
        // 菜单 / 会话点击后收起；Background 保持面板开（前端切勾后重画）
        !matches!(action, Action::Background | Action::DismissError)
    }

    fn hover(&mut self, x: f32, y: f32, inside: bool, _theme: &Theme) {
        let this = self.0.clone();
        if !inside {
            this.hovered.set(None);
            return;
        }
        let layout = this.layout.borrow();
        let next = layout.as_ref().and_then(|l| l.rows.iter().position(|row| row.rect.contains(x, y) && row.action.is_some()));
        this.hovered.set(next);
    }

    fn key(&mut self, vk: u16, _theme: &Theme) -> bool {
        let this = self.0.clone();
        // 会话列表上下键翻页；Enter 打开当前 hover 的会话
        match vk {
            0x26 | 0x28 => {
                let layout = this.layout.borrow();
                let Some(layout) = layout.as_ref() else { return false };
                let conv_rows: Vec<usize> = layout
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| matches!(row.kind, RowKind::Conversation(_)).then_some(i))
                    .collect();
                if conv_rows.is_empty() {
                    return false;
                }
                let cur = this.hovered.get();
                let next = match (vk, cur) {
                    (0x26, None) => conv_rows.last().copied(),
                    (0x26, Some(c)) => conv_rows.iter().position(|&i| i == c).and_then(|p| p.checked_sub(1)).map(|p| conv_rows[p]).or(Some(conv_rows[conv_rows.len() - 1])),
                    (0x28, None) => conv_rows.first().copied(),
                    (0x28, Some(c)) => conv_rows.iter().position(|&i| i == c).and_then(|p| conv_rows.get(p + 1)).copied().or(Some(conv_rows[0])),
                    _ => None,
                };
                if let Some(n) = next {
                    this.hovered.set(Some(n));
                }
                true
            }
            0x0D => {
                let layout = this.layout.borrow();
                let Some(layout) = layout.as_ref() else { return false };
                let Some(h) = this.hovered.get() else { return false };
                let Some(row) = layout.rows.get(h) else { return false };
                if let Some(action) = row.action {
                    drop(layout);
                    this.do_action(action);
                }
                true
            }
            _ => false,
        }
    }

    fn animate(&mut self, _dt_ms: u32, _theme: &Theme) -> bool {
        // 聊天面板没周期动画；后续闪头像预留
        false
    }

    fn is_hover_panel(&self) -> bool {
        self.0.mode.get() == Mode::Preview
    }

    fn hover_anchor(&self) -> Option<(i32, i32, i32)> {
        if self.0.mode.get() == Mode::Preview {
            let anchor = self.0.anchor_last.get();
            Some((anchor.0, anchor.1, 24))
        } else {
            None
        }
    }
}

// ---------- 后台数据 ----------

async fn load_snapshot(app: &tauri::AppHandle, bot_id: &str, qq: &str) -> Snapshot {
    let state = app.state::<crate::AppState>();
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
            is_group: matches!(c.kind, ncd_domain::chat_archive::ChatArchiveConversationKind::Group),
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

/// 右键菜单入口（menu=true）；悬停预览入口（menu=false）。
pub fn show(app: &tauri::AppHandle, bot: String, qq: String, anchor: (i32, i32), menu: bool) {
    let app_owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(content) = content() {
            content.show(bot, qq, anchor, if menu { Mode::Menu } else { Mode::Preview });
            return;
        }
        let content = Rc::new(ChatTrayPanelNative::new(app_owned.clone()));
        let cell = Box::new(ChatTrayPanelCell(content.clone())) as Box<dyn PanelContent>;
        match NativePanel::create(anchor, WIDTH, cell) {
            Ok(panel) => {
                content.set_panel(Rc::new(panel));
                CONTENT.with(|slot| *slot.borrow_mut() = Some(content.clone()));
                content.show(bot, qq, anchor, if menu { Mode::Menu } else { Mode::Preview });
            }
            Err(e) => tracing::warn!("native chat tray create: {e}"),
        }
    });
}

/// 收起面板（仅当同一代次）。
pub fn hide(app: &tauri::AppHandle) {
    let _ = app.run_on_main_thread(move || {
        with_content(|c| {
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

/// 主窗把面板配色推过来时叫。
pub fn sync_theme(app: &tauri::AppHandle) {
    let _ = app.run_on_main_thread(move || {
        with_content(|c| {
            if let Some(panel) = c.panel() {
                panel.refresh_theme();
            }
            c.mark_dirty();
        });
    });
}