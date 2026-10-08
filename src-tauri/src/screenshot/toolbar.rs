// 截图浮动工具栏的布局、命中和原生绘制。
use super::model::{Editor, PixelRect, Point, Tool};
use crate::native_panel::gfx::{Canvas, Rect, TextStyle};
use crate::native_panel::icons::Icon;
use crate::native_panel::theme::{Rgba, Theme};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Tool(Tool),
    Color([u8; 4]),
    Width(f32),
    FullScreen,
    Long,
    Undo,
    Redo,
    Copy,
    Save,
    Cancel,
    Finish,
}

pub const COLORS: [[u8; 4]; 7] = [
    [242, 79, 92, 255],
    [255, 155, 54, 255],
    [255, 220, 65, 255],
    [67, 193, 126, 255],
    [62, 156, 244, 255],
    [170, 112, 236, 255],
    [255, 255, 255, 255],
];
const TOOLS: [Tool; 8] = [
    Tool::Select,
    Tool::Rectangle,
    Tool::Ellipse,
    Tool::Arrow,
    Tool::Pen,
    Tool::Text,
    Tool::Number,
    Tool::Mosaic,
];

pub struct Toolbar {
    pub bounds: Rect,
    pub scale: f32,
    pub buttons: Vec<(Rect, Action)>,
}

impl Toolbar {
    pub fn new(selection: PixelRect, monitor: PixelRect, scale: f32) -> Self {
        let scale = scale
            .clamp(1.0, 3.0)
            .min((monitor.width as f32 - 16.0).max(1.0) / 614.0);
        let w = 614.0 * scale;
        let h = 88.0 * scale;
        let x = (selection.right() as f32 - w).clamp(
            monitor.left as f32 + 8.0,
            (monitor.right() as f32 - w - 8.0).max(monitor.left as f32 + 8.0),
        );
        let below = selection.bottom() as f32 + 12.0 * scale;
        let y = if below + h + 8.0 <= monitor.bottom() as f32 {
            below
        } else {
            selection.top as f32 - h - 12.0 * scale
        };
        let y = y.clamp(
            monitor.top as f32 + 8.0,
            (monitor.bottom() as f32 - h - 8.0).max(monitor.top as f32 + 8.0),
        );
        let bounds = Rect::new(x, y, w, h);
        let mut buttons = Vec::new();
        let mut left = x + 10.0 * scale;
        for tool in TOOLS {
            buttons.push((
                Rect::new(left, y + 8.0 * scale, 32.0 * scale, 32.0 * scale),
                Action::Tool(tool),
            ));
            left += 34.0 * scale;
        }
        left += 8.0 * scale;
        for action in [
            Action::FullScreen,
            Action::Long,
            Action::Undo,
            Action::Redo,
            Action::Copy,
            Action::Save,
            Action::Cancel,
        ] {
            buttons.push((
                Rect::new(left, y + 8.0 * scale, 32.0 * scale, 32.0 * scale),
                action,
            ));
            left += 34.0 * scale;
        }
        buttons.push((
            Rect::new(
                left + 4.0 * scale,
                y + 8.0 * scale,
                70.0 * scale,
                32.0 * scale,
            ),
            Action::Finish,
        ));
        for (i, color) in COLORS.into_iter().enumerate() {
            buttons.push((
                Rect::new(
                    x + (12.0 + i as f32 * 25.0) * scale,
                    y + 53.0 * scale,
                    21.0 * scale,
                    21.0 * scale,
                ),
                Action::Color(color),
            ));
        }
        for (i, width) in [2.0, 4.0, 8.0].into_iter().enumerate() {
            buttons.push((
                Rect::new(
                    x + (202.0 + i as f32 * 27.0) * scale,
                    y + 50.0 * scale,
                    25.0 * scale,
                    26.0 * scale,
                ),
                Action::Width(width),
            ));
        }
        Self {
            bounds,
            scale,
            buttons,
        }
    }

    pub fn hit(&self, point: Point) -> Option<Action> {
        self.buttons
            .iter()
            .find(|(r, _)| inside(*r, point))
            .map(|(_, a)| *a)
    }

    pub fn contains(&self, point: Point) -> bool {
        inside(self.bounds, point)
    }

    pub fn paint(
        &self,
        canvas: &Canvas,
        origin: Point,
        editor: &Editor,
        hover: Option<Action>,
        theme: &Theme,
    ) {
        let s = self.scale;
        let r = local(self.bounds, origin);
        canvas.fill_round(
            Rect::new(r.x, r.y + 3.0 * s, r.w, r.h),
            13.0 * s,
            Rgba::rgb(0, 0, 0).alpha(0.2),
        );
        canvas.fill_round(r, 12.0 * s, theme.elevated);
        canvas.ring(r, 12.0 * s, theme.border_subtle);
        canvas.hline(
            r.x + 12.0 * s,
            r.right() - 12.0 * s,
            r.y + 45.0 * s,
            theme.border_subtle,
        );
        for (rect, action) in &self.buttons {
            let rect = local(*rect, origin);
            let selected = match *action {
                Action::Tool(t) => editor.tool == t,
                Action::Width(w) => (editor.stroke - w).abs() < 0.01,
                _ => false,
            };
            let enabled = match action {
                Action::Undo => editor.can_undo(),
                Action::Redo => editor.can_redo(),
                Action::Long => editor.annotations.is_empty(),
                _ => true,
            };
            if *action == Action::Finish {
                canvas.fill_round(rect, 7.0 * s, theme.brand);
                canvas.icon(
                    Icon::Check,
                    rect.x + 7.0 * s,
                    rect.y + 8.0 * s,
                    16.0 * s,
                    2.0,
                    Rgba::rgb(255, 255, 255),
                    false,
                );
                label(
                    canvas,
                    "完成",
                    rect.x + 27.0 * s,
                    rect.y + 7.0 * s,
                    12.0 * s,
                    Rgba::rgb(255, 255, 255),
                );
                continue;
            }
            if selected || hover == Some(*action) {
                canvas.fill_round(
                    rect,
                    6.0 * s,
                    if selected {
                        theme.brand_soft
                    } else {
                        theme.muted
                    },
                );
            }
            let color = if !enabled {
                theme.text_disabled
            } else if selected {
                theme.brand
            } else {
                theme.text_secondary
            };
            match *action {
                Action::Color(c) => {
                    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                    canvas.circle(cx, cy, 7.0 * s, Rgba::rgb(c[0], c[1], c[2]));
                    if editor.color == c {
                        canvas.ring(
                            Rect::new(cx - 9.0 * s, cy - 9.0 * s, 18.0 * s, 18.0 * s),
                            9.0 * s,
                            theme.text,
                        );
                    }
                }
                Action::Width(w) => canvas.circle(
                    rect.x + rect.w / 2.0,
                    rect.y + rect.h / 2.0,
                    (w / 2.0 + 1.0) * s,
                    color,
                ),
                Action::Tool(t) => tool_icon(canvas, t, rect, color, s),
                Action::FullScreen => canvas.icon(
                    Icon::Monitor,
                    rect.x + 6.0 * s,
                    rect.y + 6.0 * s,
                    20.0 * s,
                    2.0,
                    color,
                    false,
                ),
                Action::Long => canvas.icon(
                    Icon::PanelsTopLeft,
                    rect.x + 6.0 * s,
                    rect.y + 6.0 * s,
                    20.0 * s,
                    2.0,
                    color,
                    false,
                ),
                Action::Undo => {
                    canvas.icon(
                        Icon::ChevronLeft,
                        rect.x + 6.0 * s,
                        rect.y + 6.0 * s,
                        20.0 * s,
                        2.0,
                        color,
                        false,
                    );
                }
                Action::Redo => {
                    canvas.icon(
                        Icon::ChevronRight,
                        rect.x + 6.0 * s,
                        rect.y + 6.0 * s,
                        20.0 * s,
                        2.0,
                        color,
                        false,
                    );
                }
                Action::Cancel => canvas.icon(
                    Icon::X,
                    rect.x + 6.0 * s,
                    rect.y + 6.0 * s,
                    20.0 * s,
                    2.0,
                    color,
                    false,
                ),
                Action::Copy => {
                    canvas.stroke_rect(
                        Rect::new(rect.x + 11.0 * s, rect.y + 11.0 * s, 13.0 * s, 14.0 * s),
                        1.6 * s,
                        color,
                    );
                    canvas.line(
                        (rect.x + 8.0 * s, rect.y + 19.0 * s),
                        (rect.x + 8.0 * s, rect.y + 7.0 * s),
                        1.6 * s,
                        color,
                    );
                    canvas.line(
                        (rect.x + 8.0 * s, rect.y + 7.0 * s),
                        (rect.x + 19.0 * s, rect.y + 7.0 * s),
                        1.6 * s,
                        color,
                    );
                }
                Action::Save => {
                    canvas.line(
                        (rect.x + 16.0 * s, rect.y + 6.0 * s),
                        (rect.x + 16.0 * s, rect.y + 20.0 * s),
                        1.7 * s,
                        color,
                    );
                    canvas.line(
                        (rect.x + 11.0 * s, rect.y + 15.0 * s),
                        (rect.x + 16.0 * s, rect.y + 20.0 * s),
                        1.7 * s,
                        color,
                    );
                    canvas.line(
                        (rect.x + 21.0 * s, rect.y + 15.0 * s),
                        (rect.x + 16.0 * s, rect.y + 20.0 * s),
                        1.7 * s,
                        color,
                    );
                    canvas.line(
                        (rect.x + 8.0 * s, rect.y + 25.0 * s),
                        (rect.x + 24.0 * s, rect.y + 25.0 * s),
                        1.7 * s,
                        color,
                    );
                }
                Action::Finish => {}
            }
        }
        let hint = hover.map(action_label).unwrap_or("Enter 完成 · Esc 取消");
        label(
            canvas,
            hint,
            r.x + 302.0 * s,
            r.y + 55.0 * s,
            11.0 * s,
            theme.text_tertiary,
        );
    }
}

pub fn label(canvas: &Canvas, text: &str, x: f32, y: f32, size: f32, color: Rgba) {
    if let Ok(text) =
        canvas
            .text_system()
            .layout(text, TextStyle::sans(size, 500, size * 1.4), 800.0)
    {
        canvas.text(&text, x, y, color);
    }
}

fn tool_icon(canvas: &Canvas, tool: Tool, r: Rect, c: Rgba, s: f32) {
    let (x, y) = (r.x + 8.0 * s, r.y + 8.0 * s);
    match tool {
        Tool::Select => {
            canvas.stroke_rect(Rect::new(x, y, 16.0 * s, 16.0 * s), 1.4 * s, c);
            canvas.fill(Rect::new(x - 2.0 * s, y - 2.0 * s, 4.0 * s, 4.0 * s), c);
        }
        Tool::Rectangle => {
            canvas.stroke_rect(Rect::new(x, y + 2.0 * s, 16.0 * s, 12.0 * s), 1.6 * s, c)
        }
        Tool::Ellipse => {
            canvas.stroke_ellipse(Rect::new(x, y + 2.0 * s, 16.0 * s, 12.0 * s), 1.6 * s, c)
        }
        Tool::Arrow => {
            canvas.line((x, y + 16.0 * s), (x + 16.0 * s, y), 1.8 * s, c);
            canvas.line((x + 7.0 * s, y), (x + 16.0 * s, y), 1.8 * s, c);
            canvas.line((x + 16.0 * s, y), (x + 16.0 * s, y + 9.0 * s), 1.8 * s, c);
        }
        Tool::Pen => {
            canvas.line((x, y + 15.0 * s), (x + 13.0 * s, y + 2.0 * s), 2.5 * s, c);
            canvas.line((x, y + 15.0 * s), (x + 5.0 * s, y + 14.0 * s), 1.4 * s, c);
        }
        Tool::Text => label(canvas, "T", x + 2.0 * s, y - 4.0 * s, 20.0 * s, c),
        Tool::Number => {
            canvas.stroke_ellipse(
                Rect::new(x - 1.0 * s, y - 1.0 * s, 18.0 * s, 18.0 * s),
                1.4 * s,
                c,
            );
            label(canvas, "1", x + 4.0 * s, y - 1.0 * s, 13.0 * s, c);
        }
        Tool::Mosaic => {
            for i in 0..3 {
                for j in 0..3 {
                    canvas.fill(
                        Rect::new(
                            x + i as f32 * 6.0 * s,
                            y + j as f32 * 6.0 * s,
                            4.0 * s,
                            4.0 * s,
                        ),
                        c.alpha(if (i + j) % 2 == 0 { 1.0 } else { 0.45 }),
                    );
                }
            }
        }
    }
}

pub fn inside(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.x < r.right() && p.y >= r.y && p.y < r.bottom()
}
pub fn local(r: Rect, origin: Point) -> Rect {
    Rect::new(r.x - origin.x, r.y - origin.y, r.w, r.h)
}
pub fn action_label(a: Action) -> &'static str {
    match a {
        Action::FullScreen => "全屏 Space",
        Action::Long => "滚动长截图 L",
        Action::Tool(Tool::Select) => "调整选区 V",
        Action::Tool(Tool::Rectangle) => "矩形 R",
        Action::Tool(Tool::Ellipse) => "椭圆 E",
        Action::Tool(Tool::Arrow) => "箭头 A",
        Action::Tool(Tool::Pen) => "画笔 P",
        Action::Tool(Tool::Text) => "文字 T",
        Action::Tool(Tool::Number) => "编号 N",
        Action::Tool(Tool::Mosaic) => "马赛克 M",
        Action::Color(_) => "标注颜色",
        Action::Width(_) => "标注粗细",
        Action::Undo => "撤销 Ctrl+Z",
        Action::Redo => "重做 Ctrl+Y",
        Action::Copy => "复制 Ctrl+C",
        Action::Save => "另存为 Ctrl+S",
        Action::Cancel => "取消 Esc",
        Action::Finish => "加入聊天 Enter",
    }
}
