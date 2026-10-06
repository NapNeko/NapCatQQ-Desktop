// 两个托盘面板共用的部件，对应 src-ui/modules/tray/trayPanelParts.tsx：头部、分隔线、操作行、角标。
// 所有尺寸都是 tailwind class 换算的 DIP（1 单位 = 4px），面板宽 260。

use super::gfx::{Canvas, Rect, TextBox, TextStyle, shared};
use super::icons::Icon;
use super::theme::{Rgba, Theme};

/// tailwind 裸 rounded 固定 4px，不跟 radiusStyle 缩放。
pub const BUTTON_RADIUS: f32 = 4.0;
/// 头部 px-3 pt-2.5 pb-1：图标 24 和两行文字列（12.5 + 2 + 11）垂直居中。
pub const HEADER_H: f32 = 39.5;
/// TrayPanelSeparator：mx-2 my-0.5 border-t。
pub const SEPARATOR_H: f32 = 5.0;
/// TrayPanelAction：py-[6px] + 14 的图标。
pub const ACTION_H: f32 = 26.0;

/// 布局阶段排好的一段字：画和命中用同一份坐标。
#[derive(Clone)]
pub struct Text {
    pub tb: TextBox,
    pub x: f32,
    pub y: f32,
}

impl Text {
    pub fn right(&self) -> f32 {
        self.x + self.tb.width
    }
}

/// 排一行字，超过 max_w 尾部省略号截断。
pub fn text(s: &str, style: TextStyle, max_w: f32, x: f32, y: f32) -> Option<Text> {
    let shared = shared().ok()?;
    let tb = shared.text.layout(s, style, max_w.max(0.0)).ok()?;
    Some(Text { tb, x, y })
}

/// 在 rect 里水平垂直居中排一行字。
pub fn centered(s: &str, style: TextStyle, rect: Rect) -> Option<Text> {
    text(s, style, rect.w, 0.0, rect.y + (rect.h - style.line_height) / 2.0).map(|mut t| {
        t.x = rect.x + (rect.w - t.tb.width) / 2.0;
        t
    })
}

pub fn draw(canvas: &Canvas, t: &Option<Text>, color: Rgba) {
    if let Some(t) = t {
        canvas.text(&t.tb, t.x, t.y, color);
    }
}

/// 对齐 JS 的 `(name.trim().charAt(0) || '?').toUpperCase()`。
pub fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into())
}

/// 头部文字：标题 12.5/600 在 (46, 10)，状态行 11 在 (58, 24.5)，状态点圆心 (49, 30)。
pub struct Header {
    pub title: Option<Text>,
    pub status: Option<Text>,
}

pub const HEADER_ICON: Rect = Rect::new(12.0, 10.75, 24.0, 24.0);

impl Header {
    pub fn new(width: f32, title: &str, status: &str) -> Self {
        Self {
            title: text(
                title,
                TextStyle::sans(12.5, 600, 12.5),
                width - 12.0 - 46.0,
                46.0,
                10.0,
            ),
            status: text(
                status,
                TextStyle::sans(11.0, 400, 11.0),
                width - 12.0 - 58.0,
                58.0,
                24.5,
            ),
        }
    }

    /// 图标由调用方先画好；这里画文字和状态点。
    pub fn paint(&self, canvas: &Canvas, theme: &Theme, dot: Rgba) {
        draw(canvas, &self.title, theme.text);
        canvas.circle(49.0, 30.0, 3.0, dot);
        draw(canvas, &self.status, theme.text_tertiary);
    }
}

/// 分隔线的 1px 线：占位 5（my-0.5）时线在 y + 2。
pub fn separator(canvas: &Canvas, theme: &Theme, width: f32, line_y: f32) {
    canvas.hline(8.0, width - 8.0, line_y, theme.border_subtle);
}

/// 操作行 px-2.5 py-[6px] gap-2.5：图标 14 在 (10, 6)，文字 12.5 在 (34, 6.75)，trailing 贴右 10。
pub struct ActionRow {
    pub rect: Rect,
    pub icon: Icon,
    pub label: Option<Text>,
}

impl ActionRow {
    /// trailing_w：右侧角标 / 勾的宽度，文字给它让出 trailing_w + 10。
    pub fn new(width: f32, y: f32, icon: Icon, label: &str, trailing_w: f32) -> Self {
        let rect = Rect::new(4.0, y, width - 8.0, ACTION_H);
        let reserve = if trailing_w > 0.0 { trailing_w + 10.0 } else { 0.0 };
        Self {
            rect,
            icon,
            label: text(
                label,
                TextStyle::sans(12.5, 400, 12.5),
                rect.w - 34.0 - 10.0 - reserve,
                rect.x + 34.0,
                y + 6.75,
            ),
        }
    }

    /// hover / 键盘选中时底色 brand-soft，图标和文字都变 brand。
    pub fn paint(&self, canvas: &Canvas, theme: &Theme, hot: bool) {
        let (icon_c, text_c) = if hot {
            canvas.fill_round(self.rect, theme.radius_md, theme.brand_soft);
            (theme.brand, theme.brand)
        } else {
            (theme.text_secondary, theme.text)
        };
        canvas.icon(
            self.icon,
            self.rect.x + 10.0,
            self.rect.y + 6.0,
            14.0,
            1.9,
            icon_c,
            false,
        );
        draw(canvas, &self.label, text_c);
    }

    /// trailing 的右缘。
    pub fn trailing_right(&self) -> f32 {
        self.rect.right() - 10.0
    }
}

/// `.chat-tray-count`：min-w 14、h 14、px 4 的胶囊，10px / 500 / 行高 1。
pub struct Badge {
    pub rect: Rect,
    pub label: Option<Text>,
}

impl Badge {
    /// 右缘对齐 right，纵向以 cy 居中。
    pub fn new(count: u32, right: f32, cy: f32) -> Self {
        let s = if count > 99 {
            "99+".to_string()
        } else {
            count.to_string()
        };
        let label = text(&s, TextStyle::sans(10.0, 500, 10.0), 40.0, 0.0, cy - 5.0);
        let w = label.as_ref().map_or(14.0, |t| (t.tb.width + 8.0).max(14.0));
        let rect = Rect::new(right - w, cy - 7.0, w, 14.0);
        let label = label.map(|mut t| {
            t.x = rect.x + (rect.w - t.tb.width) / 2.0;
            t
        });
        Self { rect, label }
    }

    pub fn paint(&self, canvas: &Canvas, theme: &Theme) {
        canvas.fill_round(self.rect, 7.0, theme.danger);
        draw(canvas, &self.label, Rgba::rgb(0xff, 0xff, 0xff));
    }
}
