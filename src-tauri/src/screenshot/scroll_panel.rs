// 长截图预览卡和图标工具条，沿用普通截图的原生主题。
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

use super::model::{PixelRect, Point};
use crate::native_panel::{
    gfx::{Canvas, Rect, TextStyle},
    icons::Icon,
    theme::{Rgba, Theme},
};

const SIDE_WIDTH: f32 = 268.0;
const SIDE_HEIGHT: f32 = 370.0;
const COMPACT_WIDTH: f32 = 338.0;
const COMPACT_HEIGHT: f32 = 116.0;
pub(super) const DETAIL_SIZE: (u32, u32) = (416, 448);

#[derive(Clone, Copy)]
enum Arrangement {
    Side,
    Compact,
}

pub(super) struct ControlLayout {
    pub position: PixelRect,
    pub scale: f32,
    pub overlaps_roi: bool,
    arrangement: Arrangement,
}

pub(super) struct ControlView<'a> {
    pub preview: Option<&'a ID2D1Bitmap>,
    pub preview_size: (u32, u32),
    pub detail: Option<&'a ID2D1Bitmap>,
    pub detail_size: (u32, u32),
    pub detail_range: (u32, u32),
    pub height: u32,
    pub parts: usize,
    pub paused: bool,
    pub status: &'a str,
    pub hover: Option<usize>,
    pub can_undo: bool,
}

impl ControlLayout {
    pub fn new(roi: PixelRect, monitor: PixelRect, scale: f32) -> Self {
        let scale = if scale.is_finite() {
            scale.clamp(1.0, 3.0)
        } else {
            1.0
        };
        let side_scale = fit_scale(scale, SIDE_WIDTH, SIDE_HEIGHT, monitor);
        let (width, height) = dimensions(SIDE_WIDTH, SIDE_HEIGHT, side_scale);
        let gap = (12.0 * side_scale).ceil() as i32;
        let margin = 8;
        let top = clamp_position(
            roi.top,
            monitor.top + margin,
            monitor.bottom() - height as i32 - margin,
        );
        for left in [roi.right() + gap, roi.left - width as i32 - gap] {
            if left >= monitor.left + margin && left + width as i32 <= monitor.right() - margin {
                let position = PixelRect {
                    left,
                    top,
                    width,
                    height,
                };
                if position.intersection(roi).is_none() {
                    return Self {
                        position,
                        scale: side_scale,
                        overlaps_roi: false,
                        arrangement: Arrangement::Side,
                    };
                }
            }
        }

        let scale = fit_scale(scale, COMPACT_WIDTH, COMPACT_HEIGHT, monitor);
        let (width, height) = dimensions(COMPACT_WIDTH, COMPACT_HEIGHT, scale);
        let gap = (12.0 * scale).ceil() as i32;
        let left = clamp_position(
            roi.right() - width as i32,
            monitor.left + margin,
            monitor.right() - width as i32 - margin,
        );
        for top in [roi.bottom() + gap, roi.top - height as i32 - gap] {
            if top >= monitor.top + margin && top + height as i32 <= monitor.bottom() - margin {
                let position = PixelRect {
                    left,
                    top,
                    width,
                    height,
                };
                if position.intersection(roi).is_none() {
                    return Self {
                        position,
                        scale,
                        overlaps_roi: false,
                        arrangement: Arrangement::Compact,
                    };
                }
            }
        }

        // 选区几乎铺满显示器时挑遮挡最少的角；采集端据此临时隐藏控制窗口。
        let right = (monitor.right() - width as i32 - margin).max(monitor.left + margin);
        let bottom = (monitor.bottom() - height as i32 - margin).max(monitor.top + margin);
        let position = [
            (right, bottom),
            (monitor.left + margin, bottom),
            (right, monitor.top + margin),
            (monitor.left + margin, monitor.top + margin),
        ]
        .into_iter()
        .map(|(left, top)| PixelRect {
            left,
            top,
            width,
            height,
        })
        .min_by_key(|position| {
            position
                .intersection(roi)
                .map_or(0, |r| u64::from(r.width) * u64::from(r.height))
        })
        .unwrap_or(PixelRect {
            left,
            top: monitor.top + margin,
            width,
            height,
        });
        Self {
            position,
            scale,
            overlaps_roi: position.intersection(roi).is_some(),
            arrangement: Arrangement::Compact,
        }
    }

    pub fn hit(&self, point: Point, can_undo: bool) -> Option<usize> {
        self.buttons()
            .into_iter()
            .find(|(r, action)| r.contains(point.x, point.y) && (*action != 2 || can_undo))
            .map(|(_, action)| action)
    }

    pub fn paint(&self, canvas: &Canvas, view: ControlView<'_>, theme: &Theme) {
        let s = self.scale;
        let bounds = Rect::new(
            0.5,
            0.5,
            self.position.width as f32 - 1.0,
            self.position.height as f32 - 1.0,
        );
        canvas.clear(theme.elevated);
        canvas.fill_round(bounds, 12.0 * s, theme.elevated);
        canvas.ring(bounds, 12.0 * s, theme.border_subtle);
        let hint = match self.arrangement {
            Arrangement::Side => {
                text(
                    canvas,
                    "长截图",
                    Rect::new(14.0 * s, 11.0 * s, 100.0 * s, 20.0 * s),
                    12.0 * s,
                    theme.text,
                );
                canvas.circle(
                    76.0 * s,
                    20.0 * s,
                    2.5 * s,
                    if view.paused {
                        theme.text_tertiary
                    } else {
                        theme.success
                    },
                );
                text(
                    canvas,
                    "局部预览",
                    Rect::new(14.0 * s, 270.0 * s, 70.0 * s, 18.0 * s),
                    10.0 * s,
                    theme.text_tertiary,
                );
                right_text(
                    canvas,
                    &format!("{} px · {} 段", view.height, view.parts),
                    254.0 * s,
                    270.0 * s,
                    10.0 * s,
                    theme.text_secondary,
                );
                let detail = Rect::new(14.0 * s, 38.0 * s, 208.0 * s, 224.0 * s);
                paint_preview(canvas, view.detail, view.detail_size, detail, theme, s);
                self.paint_overview(
                    canvas,
                    &view,
                    Rect::new(230.0 * s, 38.0 * s, 24.0 * s, 224.0 * s),
                    theme,
                );
                canvas.hline(14.0 * s, 254.0 * s, 290.0 * s, theme.border_subtle);
                Rect::new(14.0 * s, 336.0 * s, 240.0 * s, 28.0 * s)
            }
            Arrangement::Compact => {
                self.paint_overview(
                    canvas,
                    &view,
                    Rect::new(12.0 * s, 12.0 * s, 52.0 * s, 88.0 * s),
                    theme,
                );
                text(
                    canvas,
                    "长截图",
                    Rect::new(76.0 * s, 10.0 * s, 70.0 * s, 20.0 * s),
                    12.0 * s,
                    theme.text,
                );
                right_text(
                    canvas,
                    &format!("{} px · {} 段", view.height, view.parts),
                    324.0 * s,
                    12.0 * s,
                    10.0 * s,
                    theme.text_tertiary,
                );
                Rect::new(76.0 * s, 80.0 * s, 248.0 * s, 30.0 * s)
            }
        };
        for (rect, action) in self.buttons() {
            paint_button(canvas, rect, action, &view, theme, s);
        }
        let (separator, y) = match self.arrangement {
            Arrangement::Side => (86.0 * s, 306.0 * s),
            Arrangement::Compact => (148.0 * s, 48.0 * s),
        };
        canvas.line(
            (separator, y),
            (separator, y + 16.0 * s),
            1.0,
            theme.border_subtle,
        );
        let hint_text = view
            .hover
            .map(|action| action_hint(action, view.paused))
            .unwrap_or_else(|| {
                if view.status.is_empty() {
                    if view.paused {
                        "已暂停"
                    } else {
                        "上下滚动 · Enter 完成"
                    }
                } else {
                    view.status
                }
            });
        text(canvas, hint_text, hint, 10.0 * s, theme.text_secondary);
    }

    fn buttons(&self) -> [(Rect, usize); 6] {
        let s = self.scale;
        let buttons = match self.arrangement {
            Arrangement::Side => [
                (12.0, 298.0, 32.0, 32.0, 1),
                (48.0, 298.0, 32.0, 32.0, 2),
                (94.0, 298.0, 32.0, 32.0, 3),
                (130.0, 298.0, 32.0, 32.0, 4),
                (234.0, 8.0, 24.0, 24.0, 6),
                (188.0, 298.0, 66.0, 32.0, 5),
            ],
            Arrangement::Compact => [
                (76.0, 40.0, 32.0, 32.0, 1),
                (110.0, 40.0, 32.0, 32.0, 2),
                (154.0, 40.0, 32.0, 32.0, 3),
                (188.0, 40.0, 32.0, 32.0, 4),
                (222.0, 40.0, 32.0, 32.0, 6),
                (256.0, 40.0, 68.0, 32.0, 5),
            ],
        };
        buttons.map(|(x, y, width, height, action)| {
            (Rect::new(x * s, y * s, width * s, height * s), action)
        })
    }

    fn paint_overview(&self, canvas: &Canvas, view: &ControlView<'_>, area: Rect, theme: &Theme) {
        let s = self.scale;
        paint_preview(canvas, view.preview, view.preview_size, area, theme, s);
        if view.preview.is_some()
            && view.detail_range.1 - view.detail_range.0 < view.height
            && let Some(image) = fit_preview(view.preview_size, area.inset(1.0, 1.0))
        {
            let range = preview_range(image, view.detail_range, view.height, s);
            canvas.fill_round(range, 2.0 * s, theme.brand.alpha(0.12));
            canvas.ring(range, 2.0 * s, theme.brand.alpha(0.8));
        }
    }
}

fn paint_preview(
    canvas: &Canvas,
    bitmap: Option<&ID2D1Bitmap>,
    size: (u32, u32),
    area: Rect,
    theme: &Theme,
    scale: f32,
) {
    canvas.fill_round(area, 4.0 * scale, theme.muted.mix(theme.elevated, 0.4));
    if let Some(bitmap) = bitmap {
        let _clip = canvas.clip(area.inset(1.0, 1.0));
        if let Some(target) = fit_preview(size, area.inset(1.0, 1.0)) {
            canvas.draw_bitmap(bitmap, target, None, false);
        }
    }
    canvas.ring(area, 4.0 * scale, theme.border_subtle);
}

fn preview_range(image: Rect, range: (u32, u32), height: u32, scale: f32) -> Rect {
    let height = height.max(1) as f32;
    let top = image.y + image.h * range.0 as f32 / height;
    let bottom = image.y + image.h * range.1 as f32 / height;
    let h = (bottom - top).max(3.0 * scale).min(image.h);
    Rect::new(image.x, top.min(image.bottom() - h), image.w, h)
}

fn fit_scale(preferred: f32, width: f32, height: f32, monitor: PixelRect) -> f32 {
    preferred
        .min((monitor.width as f32 - 16.0).max(1.0) / width)
        .min((monitor.height as f32 - 16.0).max(1.0) / height)
}

fn dimensions(width: f32, height: f32, scale: f32) -> (u32, u32) {
    (
        (width * scale).ceil().max(1.0) as u32,
        (height * scale).ceil().max(1.0) as u32,
    )
}

fn clamp_position(value: i32, minimum: i32, maximum: i32) -> i32 {
    value.clamp(minimum, maximum.max(minimum))
}

fn fit_preview(size: (u32, u32), area: Rect) -> Option<Rect> {
    if size.0 == 0 || size.1 == 0 || area.w <= 0.0 || area.h <= 0.0 {
        return None;
    }
    let scale = (area.w / size.0 as f32).min(area.h / size.1 as f32);
    let (width, height) = (size.0 as f32 * scale, size.1 as f32 * scale);
    Some(Rect::new(
        area.x + (area.w - width) / 2.0,
        area.y + (area.h - height) / 2.0,
        width,
        height,
    ))
}

fn text(canvas: &Canvas, value: &str, bounds: Rect, size: f32, color: Rgba) {
    let _clip = canvas.clip(bounds);
    let style = TextStyle::sans(size, 500, size * 1.4);
    let rows = (bounds.h / style.line_height).floor().max(1.0) as usize;
    let mut remaining = value;
    for row in 0..rows {
        if remaining.is_empty() {
            break;
        }
        let end = if row + 1 == rows {
            remaining.len()
        } else {
            line_end(canvas, remaining, style, bounds.w)
        };
        if let Ok(layout) = canvas
            .text_system()
            .layout(&remaining[..end], style, bounds.w)
        {
            canvas.text(
                &layout,
                bounds.x,
                bounds.y + row as f32 * style.line_height,
                color,
            );
        }
        remaining = remaining[end..].trim_start();
    }
}

fn line_end(canvas: &Canvas, value: &str, style: TextStyle, width: f32) -> usize {
    let ends: Vec<_> = value
        .char_indices()
        .map(|(index, ch)| index + ch.len_utf8())
        .collect();
    let (mut first, mut last) = (0, ends.len());
    while first < last {
        let middle = first + (last - first) / 2;
        let fits = canvas
            .text_system()
            .layout(&value[..ends[middle]], style, 32_768.0)
            .is_ok_and(|line| line.width <= width);
        if fits {
            first = middle + 1;
        } else {
            last = middle;
        }
    }
    ends.get(first.saturating_sub(1))
        .copied()
        .unwrap_or(value.len())
}

fn right_text(canvas: &Canvas, value: &str, right: f32, y: f32, size: f32, color: Rgba) {
    if let Ok(layout) = canvas.text_system().layout(
        value,
        TextStyle::sans(size, 500, size * 1.4),
        180.0 * size / 11.0,
    ) {
        canvas.text(&layout, right - layout.width, y, color);
    }
}

fn action_hint(action: usize, paused: bool) -> &'static str {
    match action {
        1 if paused => "继续采集",
        1 => "暂停采集",
        2 => "撤销上一段 Ctrl+Z",
        3 => "复制",
        4 => "另存为",
        5 => "加入聊天 Enter",
        6 => "取消 Esc",
        _ => "",
    }
}

fn paint_button(
    canvas: &Canvas,
    rect: Rect,
    action: usize,
    view: &ControlView<'_>,
    theme: &Theme,
    s: f32,
) {
    let hover = view.hover == Some(action);
    let enabled = action != 2 || view.can_undo;
    if action == 5 {
        canvas.fill_round(
            rect.inset(0.0, 2.0 * s),
            6.0 * s,
            if hover {
                theme.brand_soft.mix(theme.brand, 0.12)
            } else {
                theme.brand_soft
            },
        );
        canvas.icon(
            Icon::Check,
            rect.x + 9.0 * s,
            rect.y + 9.0 * s,
            14.0 * s,
            2.0,
            theme.brand,
            false,
        );
        text(
            canvas,
            "完成",
            Rect::new(rect.x + 27.0 * s, rect.y + 7.0 * s, 35.0 * s, 18.0 * s),
            11.0 * s,
            theme.brand,
        );
        return;
    }
    let selected = action == 1 && view.paused;
    if selected || (hover && enabled) {
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
    // 维持点击面积，图形缩到与普通截图工具条相同的视觉重量。
    let s = s * 0.8;
    let (x, y) = (
        rect.x + (rect.w - 32.0 * s) / 2.0,
        rect.y + (rect.h - 32.0 * s) / 2.0,
    );
    match action {
        1 if view.paused => canvas.icon(
            Icon::Play,
            x + 7.0 * s,
            y + 7.0 * s,
            18.0 * s,
            2.0,
            color,
            false,
        ),
        1 => {
            for dx in [12.0, 20.0] {
                canvas.line(
                    (x + dx * s, y + 9.0 * s),
                    (x + dx * s, y + 23.0 * s),
                    2.0 * s,
                    color,
                );
            }
        }
        2 => canvas.icon(
            Icon::ChevronLeft,
            x + 6.0 * s,
            y + 6.0 * s,
            20.0 * s,
            2.0,
            color,
            false,
        ),
        3 => {
            canvas.stroke_rect(
                Rect::new(x + 11.0 * s, y + 11.0 * s, 13.0 * s, 14.0 * s),
                1.6 * s,
                color,
            );
            canvas.line(
                (x + 8.0 * s, y + 19.0 * s),
                (x + 8.0 * s, y + 7.0 * s),
                1.6 * s,
                color,
            );
            canvas.line(
                (x + 8.0 * s, y + 7.0 * s),
                (x + 19.0 * s, y + 7.0 * s),
                1.6 * s,
                color,
            );
        }
        4 => {
            canvas.line(
                (x + 16.0 * s, y + 6.0 * s),
                (x + 16.0 * s, y + 20.0 * s),
                1.7 * s,
                color,
            );
            canvas.line(
                (x + 11.0 * s, y + 15.0 * s),
                (x + 16.0 * s, y + 20.0 * s),
                1.7 * s,
                color,
            );
            canvas.line(
                (x + 21.0 * s, y + 15.0 * s),
                (x + 16.0 * s, y + 20.0 * s),
                1.7 * s,
                color,
            );
            canvas.line(
                (x + 8.0 * s, y + 25.0 * s),
                (x + 24.0 * s, y + 25.0 * s),
                1.7 * s,
                color,
            );
        }
        6 => canvas.icon(
            Icon::X,
            x + 6.0 * s,
            y + 6.0 * s,
            20.0 * s,
            2.0,
            color,
            false,
        ),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_uses_available_side_then_below_without_covering_capture() {
        let monitor = PixelRect {
            left: -1920,
            top: -100,
            width: 1920,
            height: 1080,
        };
        let roi = PixelRect {
            left: -1800,
            top: 10,
            width: 900,
            height: 700,
        };
        let side = ControlLayout::new(roi, monitor, 1.5);
        assert!(side.position.left > roi.right());
        assert!(!side.overlaps_roi);
        let roi = PixelRect {
            left: -1880,
            top: 10,
            width: 1800,
            height: 500,
        };
        let compact = ControlLayout::new(roi, monitor, 1.5);
        assert!(compact.position.top > roi.bottom());
        assert!(!compact.overlaps_roi);
    }

    #[test]
    fn full_monitor_capture_reports_overlap_and_preview_keeps_aspect() {
        let monitor = PixelRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        };
        let layout = ControlLayout::new(monitor, monitor, 1.0);
        assert!(layout.overlaps_roi);
        let preview = fit_preview((96, 160), Rect::new(0.0, 0.0, 232.0, 136.0)).unwrap();
        assert!((preview.w / preview.h - 0.6).abs() < 0.001);
        assert_eq!(preview.h, 136.0);
    }

    #[test]
    fn overview_marker_stays_inside_image_at_either_end() {
        for scale in [1.0, 1.5, 2.0, 3.0] {
            let image = Rect::new(14.0 * scale, 38.0 * scale, 24.0 * scale, 224.0 * scale);
            for range in [(0, 100), (23900, 24000), (0, 24000)] {
                let marker = preview_range(image, range, 24000, scale);
                assert!(marker.y >= image.y);
                assert!(marker.bottom() <= image.bottom() + 0.001);
                assert!(marker.h >= 3.0 * scale);
            }
        }
    }

    #[test]
    fn optional_native_scroll_panel_preview() {
        let Some(directory) = std::env::var_os("NCD_SCREENSHOT_SHOTS_DIR") else {
            return;
        };
        let _resources = super::super::render::SharedScope;
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let stitcher =
            super::super::stitch::Stitcher::new(super::super::stitch_native::document(480, 2480))
                .unwrap();
        let overview = stitcher.thumbnail();
        let (detail, range) = stitcher.detail_preview(DETAIL_SIZE.0, DETAIL_SIZE.1);
        for (name, arrangement, width, height, scale, status) in [
            (
                "screenshot-scroll-panel-native.png",
                Arrangement::Side,
                SIDE_WIDTH,
                SIDE_HEIGHT,
                2.0,
                "上下滚动 · Enter 完成",
            ),
            (
                "screenshot-scroll-panel-compact-native.png",
                Arrangement::Compact,
                COMPACT_WIDTH,
                COMPACT_HEIGHT,
                2.0,
                "上下滚动 · Enter 完成",
            ),
            (
                "screenshot-scroll-panel-warning-native.png",
                Arrangement::Side,
                SIDE_WIDTH,
                SIDE_HEIGHT,
                1.0,
                "重叠不足，请往回滚一点",
            ),
        ] {
            let (width, height) = dimensions(width, height, scale);
            let panel = ControlLayout {
                position: PixelRect {
                    left: 0,
                    top: 0,
                    width,
                    height,
                },
                scale,
                overlaps_roi: false,
                arrangement,
            };
            let rgba = super::super::capture::render_offscreen(width, height, |target| {
                let canvas = Canvas::new(
                    target,
                    crate::native_panel::gfx::shared().map_err(|e| e.to_string())?,
                    1.0,
                )
                .map_err(|e| e.to_string())?;
                let bitmap = canvas
                    .create_bgra_bitmap(overview.width, overview.height, &overview.bgra)
                    .map_err(|e| e.to_string())?;
                let detail_bitmap = canvas
                    .create_bgra_bitmap(detail.width, detail.height, &detail.bgra)
                    .map_err(|e| e.to_string())?;
                let frame = canvas.begin_draw();
                panel.paint(
                    &canvas,
                    ControlView {
                        preview: Some(&bitmap),
                        preview_size: (overview.width, overview.height),
                        detail: Some(&detail_bitmap),
                        detail_size: (detail.width, detail.height),
                        detail_range: range,
                        height: 2480,
                        parts: 5,
                        paused: false,
                        status,
                        hover: None,
                        can_undo: true,
                    },
                    &crate::native_panel::theme::theme(),
                );
                frame.finish().map_err(|e| e.to_string())
            })
            .unwrap();
            let path = directory.join(name);
            image::save_buffer(&path, &rgba, width, height, image::ColorType::Rgba8).unwrap();
            println!("Native scroll panel preview: {}", path.display());
        }
    }
}
