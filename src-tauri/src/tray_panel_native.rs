// 主托盘面板的原生实现：右键弹「Bot 卡片 + 全局操作」。
//
// 布局只在 measure 时计算一次并缓存成 [`Layout`]，paint / click / hover 全部读同
// 一份缓存的三个数组：行盒 (RowKind)、文字排版、命中矩形，三处永远对齐。
// 数据在打开前一次性拉成 Snapshot，面板只画快照；打开当中后端事件也会让内容层
// 重取一轮，避免面板开着却还在画旧状态。
//
// 线程模型：TrayPanelNative 只在主线程存在（thread_local 单例）。
// 对外入口 open / refresh_if_visible / sync_theme 都从任意线程
// 用 `app.run_on_main_thread` 切到主线程后再操作。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tauri::Manager;

use crate::native_panel::gfx::{Canvas, Image, Rect, TextBox, TextStyle, shared};
use crate::native_panel::icons::Icon;
use crate::native_panel::theme::{Rgba, Theme};
use crate::native_panel::window::{NativePanel, PanelContent};

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
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub cards: Vec<BotCard>,
    pub order: Vec<String>,
}

impl Snapshot {
    fn bots_sorted(&self) -> Vec<&BotCard> {
        let mut indexed: Vec<(usize, &BotCard)> = self.cards.iter().enumerate().collect();
        indexed.sort_by_key(|(i, card)| {
            self.order
                .iter()
                .position(|id| id == &card.bot_id)
                .map(|rank| rank * 4096 + *i)
                .unwrap_or(usize::MAX / 2 + *i)
        });
        indexed.into_iter().map(|(_, c)| c).collect()
    }
    fn total_pages(&self) -> usize {
        self.cards.len().div_ceil(PAGE_SIZE).max(1)
    }
    fn running_count(&self) -> usize {
        self.cards.iter().filter(|c| c.state == "running").count()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
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

enum RowKind {
    Header,
    Error,
    Separator,
    Pager,
    Card(usize),
    ActionRow { icon: Icon, label: &'static str },
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

pub struct TrayPanelNative {
    app: tauri::AppHandle,
    snapshot: RefCell<Snapshot>,
    page: Cell<usize>,
    layout: RefCell<Option<Rc<Layout>>>,
    texts: RefCell<HashMap<usize, TextBox>>,
    hovered: Cell<Option<usize>>,
    pending: RefCell<std::collections::HashSet<String>>,
    error: RefCell<Option<String>>,
    spinner: Cell<f32>,
    logo: Option<Rc<Image>>,
    panel: RefCell<Option<Rc<NativePanel>>>,
}

thread_local! {
    static CONTENT: RefCell<Option<Rc<TrayPanelNative>>> = const { RefCell::new(None) };
}

fn content() -> Option<Rc<TrayPanelNative>> {
    CONTENT.with(|slot| slot.borrow().clone())
}

fn with_content(f: impl FnOnce(&Rc<TrayPanelNative>)) {
    CONTENT.with(|slot| {
        if let Some(content) = slot.borrow().as_ref() {
            f(content);
        }
    });
}

impl TrayPanelNative {
    fn new(app: tauri::AppHandle) -> Self {
        let logo = crate::window_icon::main_window_icon(&app)
            .ok()
            .map(|img| Rc::new(Image { width: img.width(), height: img.height(), rgba: img.rgba().to_vec() }));
        Self {
            app,
            snapshot: RefCell::new(Snapshot::default()),
            page: Cell::new(1),
            layout: RefCell::new(None),
            texts: RefCell::new(HashMap::new()),
            hovered: Cell::new(None),
            pending: RefCell::new(std::collections::HashSet::new()),
            error: RefCell::new(None),
            spinner: Cell::new(0.0),
            logo,
            panel: RefCell::new(None),
        }
    }

    fn set_panel(&self, panel: Rc<NativePanel>) {
        *self.panel.borrow_mut() = Some(panel);
    }

    fn panel(&self) -> Option<Rc<NativePanel>> {
        self.panel.borrow().clone()
    }

    fn apply_snapshot(&self, snapshot: Snapshot) {
        *self.snapshot.borrow_mut() = snapshot;
        self.layout.borrow_mut().take();
        self.texts.borrow_mut().clear();
        self.error.borrow_mut().take();
    }

    fn mark_dirty(&self) {
        self.layout.borrow_mut().take();
        self.texts.borrow_mut().clear();
        if let Some(panel) = self.panel() {
            panel.resize_to_content();
        }
    }

    fn mark_theme(&self) {
        if let Some(panel) = self.panel() {
            panel.refresh_theme();
        }
        self.mark_dirty();
    }

    /// 打开：拉一轮数据，回到主线程后 resize + show。
    /// 只从主线程调用。
    fn open_at(self: &Rc<Self>, anchor: (i32, i32)) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = full_snapshot(&app).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|content| {
                    content.apply_snapshot(snapshot);
                    content.page.set(1);
                    if let Some(panel) = content.panel() {
                        panel.resize_to_content();
                        panel.show_with(anchor, true, crate::native_panel::window::tray_panel_xy);
                    }
                });
            });
        });
    }

    /// 面板开着的时候刷一轮数据。只从主线程调用。
    fn refresh_if_visible(&self) {
        let Some(panel) = self.panel() else { return };
        if !panel.is_visible() {
            return;
        }
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = full_snapshot(&app).await;
            let _ = app.run_on_main_thread(move || {
                with_content(|content| {
                    content.apply_snapshot(snapshot);
                    if let Some(panel) = content.panel() {
                        panel.resize_to_content();
                    }
                });
            });
        });
    }

    fn visible_sorted(&self) -> Vec<usize> {
        let snapshot = self.snapshot.borrow();
        let current = self.page.get().clamp(1, snapshot.total_pages());
        let start = (current - 1) * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(snapshot.cards.len());
        (start..end).collect()
    }

    fn compute_layout(&self, width: f32, theme: &Theme) -> Rc<Layout> {
        let snapshot = self.snapshot.borrow();
        let total_pages = snapshot.total_pages();
        let has_bots = !snapshot.cards.is_empty();
        let error = self.error.borrow().clone().filter(|e| !e.is_empty());
        let mut rows: Vec<Row> = Vec::new();
        let mut y: f32 = 0.0;

        rows.push(Row { rect: Rect::new(0.0, 0.0, width, 39.5), action: None, kind: RowKind::Header, hover: Rgba::TRANSPARENT });
        y = 39.5;

        if error.is_some() {
            rows.push(Row { rect: Rect::new(8.0, y, width - 16.0, 20.0), action: None, kind: RowKind::Error, hover: Rgba::TRANSPARENT });
            y += 21.0;
        }

        if has_bots {
            y = separator_row(&mut rows, y, width);
            if total_pages > 1 {
                rows.push(Row { rect: Rect::new(0.0, y, width, 20.0), action: None, kind: RowKind::Pager, hover: Rgba::TRANSPARENT });
                y += 20.0 + 1.0;
            }
            y += 4.0;
            for sorted_index in self.visible_sorted() {
                rows.push(Row {
                    rect: Rect::new(8.0, y, width - 16.0, 42.0),
                    action: Some(Action::StartStop(sorted_index)),
                    kind: RowKind::Card(sorted_index),
                    hover: theme.text.mix(theme.elevated, 0.06),
                });
                y += 42.0 + 4.0;
            }
            y = separator_row(&mut rows, y, width);
        }

        y += 3.0;
        rows.push(Row {
            rect: Rect::new(4.0, y, width - 8.0, 26.0),
            action: Some(Action::ShowMain),
            kind: RowKind::ActionRow { icon: Icon::PanelsTopLeft, label: "显示主窗口" },
            hover: theme.brand_soft,
        });
        y += 28.0;
        rows.push(Row {
            rect: Rect::new(4.0, y, width - 8.0, 26.0),
            action: Some(Action::Lightweight),
            kind: RowKind::ActionRow { icon: Icon::BatteryCharging, label: "释放界面内存" },
            hover: theme.brand_soft,
        });
        y += 28.0;

        y = separator_row(&mut rows, y, width);
        y += 3.0;
        rows.push(Row {
            rect: Rect::new(4.0, y, width - 8.0, 26.0),
            action: Some(Action::Quit),
            kind: RowKind::ActionRow { icon: Icon::LogOut, label: "退出" },
            hover: theme.danger_bg,
        });
        y += 28.0;

        Rc::new(Layout { rows, height: y.max(60.0) })
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

    fn paint_card(&self, canvas: &Canvas, sorted_index: usize, rect: Rect, theme: &Theme) {
        canvas.fill_round(rect, theme.radius_sm, theme.muted);
        canvas.ring(rect, theme.radius_sm, theme.border_subtle);
        let snapshot = self.snapshot.borrow();
        let Some(card) = snapshot.cards.get(sorted_index) else { return };
        let pending = self.pending.borrow().contains(&card.bot_id);
        let running = card.state == "running";
        let transitioning = pending || matches!(card.state.as_str(), "starting" | "stopping");
        let crashed = card.state == "crashed";
        let tint = if running {
            theme.success
        } else if transitioning {
            theme.brand
        } else if crashed {
            theme.danger
        } else {
            theme.text_disabled
        };

        // 头像
        let a = Rect::new(rect.x + 8.0, rect.y + 6.0, 30.0, 30.0);
        let seed = card.qq.chars().fold(0u32, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u32)) as usize;
        let (from, to) = avatar_palette(seed);
        canvas.gradient_round(a, theme.radius_sm, from, to);
        canvas.ring(a, theme.radius_sm, theme.border_subtle.alpha(0.7));
        let letter = card.name.chars().next().unwrap_or('?').to_ascii_uppercase().to_string();
        if let Some(t) = self.layout_text(400 + sorted_index, &letter, TextStyle::sans(11.0, 600, 11.0), a.w) {
            canvas.text(&t, a.x + (a.w - t.width) / 2.0, a.y + (a.h - t.height) / 2.0, Rgba::rgb(0xff, 0xff, 0xff).alpha(0.95));
        }

        self.draw_text(canvas, 500 + sorted_index, &card.name, TextStyle::sans(12.0, 500, 14.4), rect.x + 54.0, rect.y + 9.0, theme.text);
        self.draw_text(canvas, 600 + sorted_index, &format!("QQ: {}", card.qq), TextStyle::mono(10.5, 400, 10.5), rect.x + 54.0, rect.y + 23.5, theme.text_tertiary);

        // 状态点 + 槽位图标
        let cy = rect.y + rect.h / 2.0;
        canvas.circle(rect.right() - 10.0, cy, 4.0, tint);
        if transitioning {
            canvas.icon_rotated(Icon::LoaderCircle, rect.right() - 36.0, cy - 6.5, 13.0, 1.9, theme.brand, self.spinner.get());
        } else if running {
            canvas.icon(Icon::Square, rect.right() - 35.5, cy - 5.5, 11.0, 2.0, theme.text_secondary, true);
        } else {
            canvas.icon(Icon::Play, rect.right() - 35.5, cy - 5.5, 11.0, 2.0, theme.text_secondary, true);
        }
        if running {
            canvas.icon(Icon::Globe, rect.right() - 63.5, cy - 6.5, 13.0, 1.9, theme.text_secondary, false);
            if card.is_snowluma {
                canvas.icon(Icon::Monitor, rect.right() - 91.5, cy - 6.5, 13.0, 1.9, theme.text_secondary, false);
            }
        }
    }

    fn do_action(self: &Rc<Self>, action: Action) {
        let app = self.app.clone();
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
            Action::StartStop(sorted_index) => {
                let card = self.snapshot.borrow().cards.get(sorted_index).cloned();
                let Some(card) = card else { return };
                let running_now = card.state == "running";
                let bot_id = card.bot_id.clone();
                self.pending.borrow_mut().insert(bot_id.clone());
                self.mark_dirty();
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app2.state::<crate::AppState>();
                    let result: Result<(), String> = async {
                        if running_now {
                            state.bot_manager.stop_bot(&ncd_domain::BotId::new(bot_id.clone())).await.map(|_| ()).map_err(|e| e.to_string())
                        } else {
                            crate::commands::bot::ensure_desktop_consent(&state)?;
                            state.migrate_gate.ensure_idle()?;
                            state.bot_manager.start_bot(&ncd_domain::BotId::new(bot_id.clone())).await.map(|_| ()).map_err(|e| e.to_string())
                        }
                    }
                    .await;
                    let msg = result.err();
                    let _ = app2.run_on_main_thread(move || {
                        with_content(|content| {
                            content.pending.borrow_mut().remove(&bot_id);
                            if let Some(m) = msg {
                                let verb = if running_now { "停止" } else { "启动" };
                                *content.error.borrow_mut() = Some(format!("{verb}失败：{m}"));
                            }
                            content.mark_dirty();
                        });
                    });
                });
            }
            Action::OpenWebui(sorted_index) => {
                let card = self.snapshot.borrow().cards.get(sorted_index).cloned();
                let Some(card) = card else { return };
                let bot_id = card.bot_id.clone();
                tauri::async_runtime::spawn(async move {
                    open_napcat_webui(&app, &bot_id).await;
                });
            }
            Action::OpenNovnc(sorted_index) => {
                let card = self.snapshot.borrow().cards.get(sorted_index).cloned();
                let Some(card) = card else { return };
                let bot_id = card.bot_id.clone();
                tauri::async_runtime::spawn(async move {
                    open_snowluma_novnc(&app, &bot_id).await;
                });
            }
            Action::PagePrev => {
                let next = self.page.get().saturating_sub(1).max(1);
                if next != self.page.get() {
                    self.page.set(next);
                    self.mark_dirty();
                }
            }
            Action::PageNext => {
                let pages = self.snapshot.borrow().total_pages();
                let next = (self.page.get() + 1).min(pages);
                if next != self.page.get() {
                    self.page.set(next);
                    self.mark_dirty();
                }
            }
        }
    }
}

fn separator_row(rows: &mut Vec<Row>, y: f32, width: f32) -> f32 {
    rows.push(Row { rect: Rect::new(8.0, y, width - 16.0, 1.0), action: None, kind: RowKind::Separator, hover: Rgba::TRANSPARENT });
    y + 1.0
}

fn clone_tb(t: &TextBox) -> TextBox {
    TextBox { layout: t.layout.clone(), width: t.width, height: t.height }
}

fn avatar_palette(seed: usize) -> (Rgba, Rgba) {
    const PALETTE: [(Rgba, Rgba); 6] = [
        (Rgba::rgb(0xf9, 0xa8, 0xd4), Rgba::rgb(0xe8, 0x79, 0xf2)),
        (Rgba::rgb(0xfc, 0xd3, 0x4d), Rgba::rgb(0xfb, 0x92, 0x3c)),
        (Rgba::rgb(0x6e, 0xe7, 0xb7), Rgba::rgb(0x14, 0xb8, 0xa6)),
        (Rgba::rgb(0x7d, 0xd3, 0xfc), Rgba::rgb(0x63, 0x66, 0xf1)),
        (Rgba::rgb(0xc4, 0xb5, 0xfd), Rgba::rgb(0xe8, 0x79, 0xf2)),
        (Rgba::rgb(0xfd, 0xa4, 0xaf), Rgba::rgb(0xef, 0x43, 0x54)),
    ];
    PALETTE[seed % PALETTE.len()]
}

// ---------- PanelContent：把 Rc<TrayPanelNative> 包成 Box<dyn PanelContent> ----------

pub struct TrayPanelCell(pub Rc<TrayPanelNative>);

impl PanelContent for TrayPanelCell {
    fn measure(&mut self, width: f32, theme: &Theme) -> (f32, f32) {
        let this = self.0.clone();
        let layout = this.compute_layout(if width <= 0.0 { WIDTH } else { width }, theme);
        let height = layout.height;
        *this.layout.borrow_mut() = Some(layout);
        (WIDTH, height.ceil().max(60.0))
    }
    fn paint(&mut self, canvas: &Canvas, _x: f32, _y: f32, width: f32, theme: &Theme) {
        let this = self.0.clone();
        let layout_opt = this.layout.borrow().clone();
        let layout = layout_opt.unwrap_or_else(|| this.compute_layout(width, theme));
        let hovered = this.hovered.get();
        let snapshot = this.snapshot.borrow();
        let running = snapshot.running_count() > 0;
        let status = if snapshot.cards.is_empty() {
            "后台待命".to_string()
        } else if running {
            format!("{}/{} 个 Bot 运行中", snapshot.running_count(), snapshot.cards.len())
        } else {
            "全部已停止".to_string()
        };

        for (i, row) in layout.rows.iter().enumerate() {
            match &row.kind {
                RowKind::Header => {
                    if let Some(logo) = &this.logo {
                        canvas.image_round(logo, Rect::new(12.0, 10.75, 24.0, 24.0), theme.radius_md);
                    } else {
                        canvas.fill_round(Rect::new(12.0, 10.75, 24.0, 24.0), theme.radius_md, theme.brand_soft);
                    }
                    canvas.ring(Rect::new(12.0, 10.75, 24.0, 24.0), theme.radius_md, theme.border_subtle.alpha(0.6));
                    this.draw_text(canvas, 0, "NapCatQQ-Desktop", TextStyle::sans(12.5, 600, 12.5), 42.0, 21.0, theme.text);
                    canvas.circle(21.0, 30.5, 3.0, if running { theme.success } else { theme.text_disabled });
                    this.draw_text(canvas, 1, &status, TextStyle::sans(11.0, 400, 11.0), 28.0, 24.5, theme.text_tertiary);
                }
                RowKind::Error => {
                    canvas.fill_round(row.rect, theme.radius_sm, theme.danger_bg);
                    if let Some(err) = this.error.borrow().clone() {
                        this.draw_text(canvas, 100 + i, &err, TextStyle::sans(11.0, 400, 16.0), row.rect.x + 2.0, row.rect.y + 2.0, theme.danger);
                    }
                }
                RowKind::Separator => {
                    canvas.hline(row.rect.x, row.rect.right(), row.rect.y, theme.border_subtle);
                }
                RowKind::Pager => {
                    let total = snapshot.total_pages();
                    let current = this.page.get().clamp(1, total);
                    this.draw_text(canvas, 200 + i, &format!("机器人列表 ({})", snapshot.cards.len()), TextStyle::sans(11.0, 400, 11.0), 10.0, row.rect.y + 4.5, theme.text_tertiary);
                    let prev_c = if current > 1 { theme.text_secondary } else { theme.text_disabled.alpha(0.5) };
                    let next_c = if current < total { theme.text_secondary } else { theme.text_disabled.alpha(0.5) };
                    canvas.icon(Icon::ChevronLeft, row.rect.right() - 63.0, row.rect.y + 3.0, 14.0, 1.8, prev_c, false);
                    this.draw_text(canvas, 210 + i, &format!("{current} / {total}"), TextStyle::mono(11.0, 400, 11.0), row.rect.right() - 44.0, row.rect.y + 4.5, theme.text_secondary);
                    canvas.icon(Icon::ChevronRight, row.rect.right() - 18.0, row.rect.y + 3.0, 14.0, 1.8, next_c, false);
                }
                RowKind::Card(sorted_index) => {
                    if hovered == Some(i) && row.hover.a > 0.0 {
                        canvas.fill_round(row.rect, theme.radius_sm, row.hover);
                    }
                    this.paint_card(canvas, *sorted_index, row.rect, theme);
                }
                RowKind::ActionRow { icon, label } => {
                    if hovered == Some(i) && row.hover.a > 0.0 {
                        canvas.fill_round(row.rect, theme.radius_sm, row.hover);
                    }
                    let is_quit = matches!(*icon, Icon::LogOut);
                    let icon_y = row.rect.y + (row.rect.h - 14.0) / 2.0;
                    canvas.icon(*icon, row.rect.x + 10.0, icon_y, 14.0, 1.9, if is_quit { theme.danger } else { theme.text_secondary }, false);
                    let text_y = row.rect.y + (row.rect.h - 12.5) / 2.0 + 0.8;
                    this.draw_text(canvas, 300 + i, label, TextStyle::sans(12.5, 400, 12.5), row.rect.x + 34.0, text_y, if is_quit { theme.danger } else { theme.text });
                }
            }
        }
    }
    fn click(&mut self, x: f32, y: f32, _theme: &Theme) -> bool {
        let this = self.0.clone();
        let action: Option<Action> = (|| {
            let layout = this.layout.borrow();
            let layout = layout.as_ref()?;
            for row in &layout.rows {
                if !row.rect.contains(x, y) {
                    continue;
                }
                let rel_x = x - row.rect.x;
                let action = match &row.kind {
                    RowKind::Card(sorted_index) => {
                        let c = this.snapshot.borrow().cards.get(*sorted_index).cloned()?;
                        let running = c.state == "running";
                        let from_right = x - row.rect.right();
                        if from_right >= -36.0 && from_right < -12.0 {
                            Action::StartStop(*sorted_index)
                        } else if from_right >= -64.0 && from_right < -36.0 && running && c.webui.is_some() {
                            Action::OpenWebui(*sorted_index)
                        } else if from_right >= -92.0 && from_right < -64.0 && running && c.is_snowluma {
                            Action::OpenNovnc(*sorted_index)
                        } else {
                            return None;
                        }
                    }
                    RowKind::Pager => {
                        let total = this.snapshot.borrow().total_pages();
                        if rel_x >= row.rect.w - 64.0 && rel_x < row.rect.w - 44.0 && this.page.get() > 1 {
                            Action::PagePrev
                        } else if rel_x >= row.rect.w - 20.0 && this.page.get() < total {
                            Action::PageNext
                        } else {
                            return None;
                        }
                    }
                    RowKind::ActionRow { .. } => row.action?,
                    _ => return None,
                };
                return Some(action);
            }
            None
        })();
        let Some(action) = action else { return false };
        this.error.borrow_mut().take();
        this.do_action(action);
        matches!(action, Action::ShowMain | Action::Lightweight | Action::Quit)
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
        match vk {
            0x25 => { this.do_action(Action::PagePrev); true }
            0x27 => { this.do_action(Action::PageNext); true }
            _ => false,
        }
    }
    fn animate(&mut self, dt_ms: u32, _theme: &Theme) -> bool {
        let this = self.0.clone();
        let need = !this.pending.borrow().is_empty();
        if !need {
            return false;
        }
        let next = (this.spinner.get() + dt_ms as f32 * 0.36) % 720.0;
        this.spinner.set(next);
        true
    }
}

// ---------- 后台数据 ----------

async fn full_snapshot(app: &tauri::AppHandle) -> Snapshot {
    let state = app.state::<crate::AppState>();
    let snapshots = state.bot_manager.list_snapshots().await;
    let configs: HashMap<String, ncd_domain::BotConfig> = state
        .bot_manager
        .list_bot_configs()
        .await
        .map(|v| v.into_iter().map(|c| (c.bot.name.clone(), c)).collect())
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
            let is_snowluma = matches!(flavors.get(&bot_id), Some(ncd_domain::bot_config::BackendType::SnowLuma))
                || cfg.is_some_and(|c| matches!(c.bot.backend_type, ncd_domain::bot_config::BackendType::SnowLuma));
            BotCard {
                state: BotCard::state_str(&snap.state).to_string(),
                bot_id: bot_id.clone(),
                name: cfg.map(|c| c.bot.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| bot_id.clone()),
                qq: cfg.map(|c| c.bot.qq_id.to_string()).filter(|q| !q.is_empty() && q != "0").unwrap_or_else(|| bot_id.clone()),
                is_snowluma,
                webui: webuis.get(&bot_id).cloned(),
            }
        })
        .collect();
    Snapshot { cards, order: Vec::new() }
}

// ---------- URL / noVNC ----------

async fn open_napcat_webui(app: &tauri::AppHandle, bot_id: &str) {
    let state = app.state::<crate::AppState>();
    let bindings = state.bot_manager.list_napcat_webui_endpoints().await;
    let Some((_, port, token, _)) = bindings.into_iter().find(|(id, _, _, _)| id.as_str() == bot_id) else { return };
    let url = format!("http://127.0.0.1:{port}/webui?token={}", urlencoding::encode(&token));
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app.opener().open_url(&url, None::<&str>) {
        tracing::warn!("native tray open webui: {e}");
    }
}

async fn open_snowluma_novnc(app: &tauri::AppHandle, bot_id: &str) {
    let state = app.state::<crate::AppState>();
    let bid = ncd_domain::BotId::new(bot_id.to_string());
    let Ok(Some(cfg)) = state.bot_manager.get_bot_config(&bid).await else { return };
    if cfg.bot.backend_type != ncd_domain::BackendType::SnowLuma {
        return;
    }
    let url = if cfg.bot.deployment_type == ncd_domain::DeploymentType::Docker {
        let Some(ep) = state.bot_manager.snowluma_docker_endpoints(&bid).await else { return };
        let pwd = urlencoding::encode(&ep.vnc_password);
        format!(
            "http://127.0.0.1:{}/vnc.html?autoconnect=1&resize=scale&password={pwd}&view_only=0&reconnect=1&reconnect_delay=3000",
            ep.novnc_local_port
        )
    } else if cfg.bot.deployment_type == ncd_domain::DeploymentType::Native {
        let ncd_domain::RuntimeTarget::Server(server_id) = &cfg.bot.runtime_target else { return };
        let Some(ep) = state.bot_manager.snowluma_native_endpoints_for_server(server_id).await else { return };
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

// ---------- 对外入口：open / refresh_if_visible / sync_theme ----------

/// 托盘右键入口。任意线程可调；内部把创建与打开都切到主线程。
pub fn open(app: &tauri::AppHandle, anchor: (i32, i32)) {
    let app_owned = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(content) = content() {
            content.open_at(anchor);
            return;
        }
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
        with_content(|content| content.mark_theme());
    });
}