// 截图预览和导出共用 Direct2D 绘制，位图只上传当前视口。

use std::cell::RefCell;

use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap, ID2D1PathGeometry1};

use super::capture::{Desktop, MonitorImage, render_offscreen};
use super::model::{Annotation, Editor, PixelRect, Point, Tool};
use crate::native_panel::gfx::{Canvas, Rect, TextBox, TextStyle, release_shared, shared};
use crate::native_panel::theme::Rgba;

const MOSAIC_CELL: u32 = 12;
const MAX_EXPORT_BYTES: usize = 256 * 1024 * 1024;

pub(crate) struct SharedScope;

impl Drop for SharedScope {
    fn drop(&mut self) {
        // Windows 字体资源在当前调用结束时释放，避免留给线程局部析构阶段。
        release_shared();
    }
}

struct Tile {
    bounds: PixelRect,
    bitmap: ID2D1Bitmap,
    mosaic_bounds: PixelRect,
    mosaic: ID2D1Bitmap,
}

#[derive(Default)]
struct Prepared {
    path: Option<ID2D1PathGeometry1>,
    text: Vec<TextBox>,
}

#[derive(Default)]
struct PreparedCache {
    revision: u64,
    items: Vec<Prepared>,
}

pub struct Scene {
    tiles: Vec<Tile>,
    prepared: RefCell<PreparedCache>,
}

impl Scene {
    pub fn new(canvas: &Canvas, desktop: &Desktop, viewport: PixelRect) -> Result<Self, String> {
        let mut tiles = Vec::new();
        for monitor in &desktop.monitors {
            let Some(bounds) = monitor.bounds.intersection(viewport) else {
                continue;
            };
            let bitmap = if bounds == monitor.bounds {
                canvas.create_bgra_bitmap(bounds.width, bounds.height, &monitor.bgra)
            } else {
                let cropped = crop_bgra(monitor, bounds)?;
                canvas.create_bgra_bitmap(bounds.width, bounds.height, &cropped)
            }
            .map_err(|error| format!("创建截图位图失败：{error}"))?;
            let (mosaic_bounds, width, height, pixels) = mosaic_pixels(monitor, bounds)?;
            let mosaic = canvas
                .create_bgra_bitmap(width, height, &pixels)
                .map_err(|error| format!("创建马赛克位图失败：{error}"))?;
            tiles.push(Tile {
                bounds,
                bitmap,
                mosaic_bounds,
                mosaic,
            });
        }
        if tiles.is_empty() {
            return Err("选区没有覆盖显示器".into());
        }
        Ok(Self {
            tiles,
            prepared: RefCell::new(PreparedCache::default()),
        })
    }

    pub fn paint_background(&self, canvas: &Canvas, origin: Point) {
        for tile in &self.tiles {
            canvas.draw_bitmap(&tile.bitmap, local_rect(tile.bounds, origin), None, true);
        }
    }

    pub fn paint_magnifier(&self, canvas: &Canvas, point: Point, destination: Rect) {
        canvas.fill(destination, Rgba::rgb(16, 18, 22));
        let Some(tile) = self.tiles.iter().find(|tile| tile.bounds.contains(point)) else {
            return;
        };
        let Some((source, target)) = magnifier_rects(tile.bounds, point, destination) else {
            return;
        };
        canvas.draw_bitmap(&tile.bitmap, target, Some(source), true);
    }

    pub fn paint_annotations(
        &self,
        canvas: &Canvas,
        editor: &Editor,
        origin: Point,
    ) -> Result<(), String> {
        let Some(selection) = editor.selection else {
            return Ok(());
        };
        let mut cache = self.prepared.borrow_mut();
        if cache.revision != editor.revision() || cache.items.len() != editor.annotations.len() {
            cache.items = editor
                .annotations
                .iter()
                .map(|annotation| prepare(canvas, annotation))
                .collect::<Result<Vec<_>, _>>()?;
            cache.revision = editor.revision();
        }
        let _clip = canvas.clip(local_rect(selection, origin));
        for (annotation, prepared) in editor.annotations.iter().zip(&cache.items) {
            self.paint_prepared(canvas, annotation, prepared, origin);
        }
        Ok(())
    }

    pub fn paint_annotation(
        &self,
        canvas: &Canvas,
        annotation: &Annotation,
        origin: Point,
    ) -> Result<(), String> {
        let prepared = prepare(canvas, annotation)?;
        self.paint_prepared(canvas, annotation, &prepared, origin);
        Ok(())
    }

    fn paint_prepared(
        &self,
        canvas: &Canvas,
        annotation: &Annotation,
        prepared: &Prepared,
        origin: Point,
    ) {
        let Some(&first) = annotation.points.first() else {
            return;
        };
        let last = annotation.points.last().copied().unwrap_or(first);
        let from = (first.x - origin.x, first.y - origin.y);
        let to = (last.x - origin.x, last.y - origin.y);
        let rect = Rect::new(
            from.0.min(to.0),
            from.1.min(to.1),
            (from.0 - to.0).abs(),
            (from.1 - to.1).abs(),
        );
        let color = rgba(annotation.color);
        match annotation.tool {
            Tool::Select => {}
            Tool::Rectangle => canvas.stroke_rect(rect, annotation.width, color),
            Tool::Ellipse => canvas.stroke_ellipse(rect, annotation.width, color),
            Tool::Arrow => paint_arrow(canvas, from, to, annotation.width, color),
            Tool::Pen => {
                if let Some(path) = &prepared.path {
                    canvas.stroke_path(path, (-origin.x, -origin.y), annotation.width, color);
                } else {
                    canvas.circle(from.0, from.1, annotation.width / 2.0, color);
                }
            }
            Tool::Text => {
                let mut y = from.1;
                for line in &prepared.text {
                    canvas.text(line, from.0, y, color);
                    y += line.height;
                }
            }
            Tool::Number => {
                let radius = (annotation.width * 2.0 + 10.0).clamp(12.0, 32.0);
                canvas.circle(from.0, from.1, radius, color);
                if let Some(text) = prepared.text.first() {
                    let ink = if 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b > 0.65 {
                        Rgba::rgb(25, 25, 25)
                    } else {
                        Rgba::rgb(255, 255, 255)
                    };
                    canvas.text(
                        text,
                        from.0 - text.width / 2.0,
                        from.1 - text.height / 2.0,
                        ink,
                    );
                }
            }
            Tool::Mosaic => {
                let area = PixelRect::from_points(first, last);
                for tile in &self.tiles {
                    let Some(visible) = area.intersection(tile.bounds) else {
                        continue;
                    };
                    let _clip = canvas.clip(local_rect(visible, origin));
                    canvas.draw_bitmap(
                        &tile.mosaic,
                        local_rect(tile.mosaic_bounds, origin),
                        None,
                        true,
                    );
                }
            }
        }
    }
}

fn prepare(canvas: &Canvas, annotation: &Annotation) -> Result<Prepared, String> {
    let mut prepared = Prepared::default();
    match annotation.tool {
        Tool::Pen if annotation.points.len() > 1 => {
            let points: Vec<_> = annotation
                .points
                .iter()
                .map(|point| (point.x, point.y))
                .collect();
            prepared.path = Some(
                canvas
                    .path(&points)
                    .map_err(|error| format!("创建笔画失败：{error}"))?,
            );
        }
        Tool::Text => {
            let size = (annotation.width * 3.0 + 14.0).clamp(16.0, 96.0);
            let style = TextStyle::sans(size, 500, size * 1.35);
            for line in annotation.text.split('\n') {
                prepared.text.push(
                    canvas
                        .text_system()
                        .layout(line.trim_end_matches('\r'), style, 32_768.0)
                        .map_err(|error| format!("排版标注文字失败：{error}"))?,
                );
            }
        }
        Tool::Number => {
            let radius = (annotation.width * 2.0 + 10.0).clamp(12.0, 32.0);
            prepared.text.push(
                canvas
                    .text_system()
                    .layout(
                        &annotation.number.max(1).to_string(),
                        TextStyle::sans(radius * 1.05, 700, radius * 1.4),
                        radius * 2.0,
                    )
                    .map_err(|error| format!("排版序号失败：{error}"))?,
            );
        }
        _ => {}
    }
    Ok(prepared)
}

fn rgba(color: [u8; 4]) -> Rgba {
    Rgba::rgb(color[0], color[1], color[2]).alpha(f32::from(color[3]) / 255.0)
}

fn local_rect(rect: PixelRect, origin: Point) -> Rect {
    Rect::new(
        rect.left as f32 - origin.x,
        rect.top as f32 - origin.y,
        rect.width as f32,
        rect.height as f32,
    )
}

fn magnifier_rects(bounds: PixelRect, point: Point, destination: Rect) -> Option<(Rect, Rect)> {
    if !point.valid() || !bounds.contains(point) || destination.w <= 0.0 || destination.h <= 0.0 {
        return None;
    }
    let area = PixelRect {
        left: point.x.floor() as i32 - 12,
        top: point.y.floor() as i32 - 12,
        width: 25,
        height: 25,
    };
    let visible = area.intersection(bounds)?;
    let source = local_rect(visible, bounds.origin());
    // 屏幕边缘留空，保持十字中心对应鼠标下的像素，而不是把采样框平移。
    let target = Rect::new(
        destination.x + (visible.left - area.left) as f32 / 25.0 * destination.w,
        destination.y + (visible.top - area.top) as f32 / 25.0 * destination.h,
        visible.width as f32 / 25.0 * destination.w,
        visible.height as f32 / 25.0 * destination.h,
    );
    Some((source, target))
}

fn paint_arrow(canvas: &Canvas, from: (f32, f32), to: (f32, f32), width: f32, color: Rgba) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let length = dx.hypot(dy);
    if length < 1.0 {
        return;
    }
    let head = (width * 4.0 + 8.0).min(length * 0.65);
    let ux = dx / length;
    let uy = dy / length;
    canvas.line(from, to, width, color);
    canvas.line(
        to,
        (
            to.0 - ux * head + uy * head * 0.5,
            to.1 - uy * head - ux * head * 0.5,
        ),
        width,
        color,
    );
    canvas.line(
        to,
        (
            to.0 - ux * head - uy * head * 0.5,
            to.1 - uy * head + ux * head * 0.5,
        ),
        width,
        color,
    );
}

fn pixel_length(width: u32, height: u32) -> Result<usize, String> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|&bytes| bytes > 0 && bytes <= MAX_EXPORT_BYTES)
        .ok_or_else(|| "截图尺寸过大或选区为空".into())
}

fn crop_bgra(monitor: &MonitorImage, bounds: PixelRect) -> Result<Vec<u8>, String> {
    if monitor.bgra.len() != pixel_length(monitor.bounds.width, monitor.bounds.height)?
        || monitor.bounds.intersection(bounds) != Some(bounds)
    {
        return Err("显示器像素长度或裁剪范围无效".into());
    }
    let mut pixels = vec![0; pixel_length(bounds.width, bounds.height)?];
    let x = (bounds.left - monitor.bounds.left) as usize;
    let y = (bounds.top - monitor.bounds.top) as usize;
    let source_stride = monitor.bounds.width as usize * 4;
    let stride = bounds.width as usize * 4;
    for row in 0..bounds.height as usize {
        let source = (y + row) * source_stride + x * 4;
        pixels[row * stride..(row + 1) * stride]
            .copy_from_slice(&monitor.bgra[source..source + stride]);
    }
    Ok(pixels)
}

fn mosaic_pixels(
    monitor: &MonitorImage,
    bounds: PixelRect,
) -> Result<(PixelRect, u32, u32, Vec<u8>), String> {
    if monitor.bgra.len() != pixel_length(monitor.bounds.width, monitor.bounds.height)? {
        return Err("显示器像素长度无效".into());
    }
    // 网格锚定原显示器，导出局部选区时仍与预览一致。
    let start_x = (bounds.left - monitor.bounds.left) as u32 / MOSAIC_CELL;
    let start_y = (bounds.top - monitor.bounds.top) as u32 / MOSAIC_CELL;
    let end_x = ((bounds.right() - monitor.bounds.left) as u32).div_ceil(MOSAIC_CELL);
    let end_y = ((bounds.bottom() - monitor.bounds.top) as u32).div_ceil(MOSAIC_CELL);
    let width = end_x - start_x;
    let height = end_y - start_y;
    let mut pixels = Vec::with_capacity(pixel_length(width, height)?);
    for y in start_y..end_y {
        for x in start_x..end_x {
            let mut sum = [0u32; 3];
            // 每格采样九点；桌面只在创建视口时读取一次。
            for sy in [2, 6, 10] {
                for sx in [2, 6, 10] {
                    let px = (x * MOSAIC_CELL + sx).min(monitor.bounds.width - 1);
                    let py = (y * MOSAIC_CELL + sy).min(monitor.bounds.height - 1);
                    let at = (py as usize * monitor.bounds.width as usize + px as usize) * 4;
                    for channel in 0..3 {
                        sum[channel] += u32::from(monitor.bgra[at + channel]);
                    }
                }
            }
            pixels.extend_from_slice(&[
                (sum[0] / 9) as u8,
                (sum[1] / 9) as u8,
                (sum[2] / 9) as u8,
                255,
            ]);
        }
    }
    let grid_bounds = PixelRect {
        left: monitor.bounds.left + (start_x * MOSAIC_CELL) as i32,
        top: monitor.bounds.top + (start_y * MOSAIC_CELL) as i32,
        width: width * MOSAIC_CELL,
        height: height * MOSAIC_CELL,
    };
    Ok((grid_bounds, width, height, pixels))
}

fn flatten(desktop: &Desktop, selection: PixelRect) -> Result<Vec<u8>, String> {
    let mut rgba = vec![0; pixel_length(selection.width, selection.height)?];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    for monitor in &desktop.monitors {
        let Some(bounds) = monitor.bounds.intersection(selection) else {
            continue;
        };
        if monitor.bgra.len() != pixel_length(monitor.bounds.width, monitor.bounds.height)? {
            return Err("显示器像素长度无效".into());
        }
        let sx = (bounds.left - monitor.bounds.left) as usize;
        let sy = (bounds.top - monitor.bounds.top) as usize;
        let dx = (bounds.left - selection.left) as usize;
        let dy = (bounds.top - selection.top) as usize;
        for row in 0..bounds.height as usize {
            let source = ((sy + row) * monitor.bounds.width as usize + sx) * 4;
            let destination = ((dy + row) * selection.width as usize + dx) * 4;
            let bytes = bounds.width as usize * 4;
            for (bgra, rgba) in monitor.bgra[source..source + bytes]
                .chunks_exact(4)
                .zip(rgba[destination..destination + bytes].chunks_exact_mut(4))
            {
                rgba.copy_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
            }
        }
    }
    Ok(rgba)
}

pub fn export_rgba(desktop: &Desktop, editor: &Editor) -> Result<(u32, u32, Vec<u8>), String> {
    let _resources = SharedScope;
    let selection = editor
        .selection
        .ok_or_else(|| "请先选择截图区域".to_string())?;
    pixel_length(selection.width, selection.height)?;
    if desktop.bounds.intersection(selection) != Some(selection)
        || !desktop
            .monitors
            .iter()
            .any(|monitor| monitor.bounds.intersection(selection).is_some())
    {
        return Err("截图选区超出桌面范围".into());
    }
    let rgba = if editor.annotations.is_empty() {
        flatten(desktop, selection)?
    } else {
        let resources = shared().map_err(|error| format!("初始化截图绘制失败：{error}"))?;
        render_offscreen(selection.width, selection.height, |target| {
            let canvas = Canvas::new(target, resources, 1.0)
                .map_err(|error| format!("创建截图画布失败：{error}"))?;
            let scene = Scene::new(&canvas, desktop, selection)?;
            let frame = canvas.begin_draw();
            canvas.clear(Rgba::rgb(0, 0, 0));
            scene.paint_background(&canvas, selection.origin());
            scene.paint_annotations(&canvas, editor, selection.origin())?;
            frame
                .finish()
                .map_err(|error| format!("导出截图失败：{error}"))
        })?
    };
    Ok((selection.width, selection.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(bounds: PixelRect) -> MonitorImage {
        let mut bgra = Vec::with_capacity(bounds.width as usize * bounds.height as usize * 4);
        for y in 0..bounds.height {
            for x in 0..bounds.width {
                bgra.extend_from_slice(&[
                    (x % 256) as u8,
                    (y % 256) as u8,
                    ((x + y) % 256) as u8,
                    255,
                ]);
            }
        }
        MonitorImage {
            bounds,
            scale: 1.0,
            bgra,
        }
    }

    fn desktop(bounds: PixelRect) -> Desktop {
        Desktop {
            monitors: vec![monitor(bounds)],
            windows: Vec::new(),
            bounds,
        }
    }

    fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> &[u8] {
        let at = (y as usize * width as usize + x as usize) * 4;
        &rgba[at..at + 4]
    }

    fn annotation(tool: Tool, from: Point, to: Point, color: [u8; 4]) -> Annotation {
        Annotation {
            tool,
            points: vec![from, to],
            color,
            width: 3.0,
            text: String::new(),
            number: 0,
        }
    }

    #[test]
    fn cropped_bytes_use_physical_offsets_on_negative_monitor() {
        let monitor = monitor(PixelRect {
            left: -40,
            top: -15,
            width: 8,
            height: 6,
        });
        let crop = crop_bgra(
            &monitor,
            PixelRect {
                left: -38,
                top: -14,
                width: 3,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(crop.len(), 24);
        assert_eq!(&crop[..4], &[2, 1, 3, 255]);
        assert_eq!(&crop[20..], &[4, 2, 6, 255]);
        assert!(
            crop_bgra(
                &monitor,
                PixelRect {
                    left: -41,
                    top: -15,
                    width: 3,
                    height: 2
                }
            )
            .is_err()
        );
    }

    #[test]
    fn magnifier_clamps_source_and_keeps_cursor_pixel_centered_at_monitor_edge() {
        let bounds = PixelRect {
            left: -40,
            top: -15,
            width: 100,
            height: 80,
        };
        let destination = Rect::new(10.0, 20.0, 100.0, 100.0);
        let (source, target) =
            magnifier_rects(bounds, Point { x: -40.0, y: -15.0 }, destination).unwrap();
        assert_eq!(source, Rect::new(0.0, 0.0, 13.0, 13.0));
        assert_eq!(target, Rect::new(58.0, 68.0, 52.0, 52.0));
        let (source, target) =
            magnifier_rects(bounds, Point { x: -10.0, y: 15.0 }, destination).unwrap();
        assert_eq!(source, Rect::new(18.0, 18.0, 25.0, 25.0));
        assert_eq!(target, destination);
        assert!(magnifier_rects(bounds, Point { x: 60.0, y: 15.0 }, destination).is_none());
    }

    #[test]
    fn mosaic_crop_keeps_original_cell_alignment_and_source_samples() {
        let monitor = monitor(PixelRect {
            left: -25,
            top: -17,
            width: 50,
            height: 40,
        });
        let (whole, width, _, all) = mosaic_pixels(&monitor, monitor.bounds).unwrap();
        let crop = PixelRect {
            left: -10,
            top: 0,
            width: 25,
            height: 19,
        };
        let (part, part_width, part_height, pixels) = mosaic_pixels(&monitor, crop).unwrap();
        let x = ((part.left - whole.left) as u32) / MOSAIC_CELL;
        let y = ((part.top - whole.top) as u32) / MOSAIC_CELL;
        for py in 0..part_height {
            for px in 0..part_width {
                assert_eq!(
                    pixel(&pixels, part_width, px, py),
                    pixel(&all, width, px + x, py + y)
                );
            }
        }
        assert_eq!(pixel(&all, width, 0, 0), &[6, 6, 12, 255]);
    }

    #[test]
    fn unannotated_export_preserves_bytes_and_uses_opaque_black_for_monitor_gaps() {
        let mut first = monitor(PixelRect {
            left: -4,
            top: -2,
            width: 2,
            height: 2,
        });
        let mut second = monitor(PixelRect {
            left: 0,
            top: -2,
            width: 2,
            height: 2,
        });
        for px in first.bgra.chunks_exact_mut(4) {
            px.copy_from_slice(&[20, 30, 40, 255]);
        }
        for px in second.bgra.chunks_exact_mut(4) {
            px.copy_from_slice(&[50, 60, 70, 255]);
        }
        let desktop = Desktop {
            monitors: vec![first, second],
            windows: Vec::new(),
            bounds: PixelRect {
                left: -4,
                top: -2,
                width: 6,
                height: 2,
            },
        };
        let mut editor = Editor::new();
        editor.selection = Some(PixelRect {
            left: -3,
            top: -2,
            width: 5,
            height: 2,
        });
        let (width, height, rgba) = export_rgba(&desktop, &editor).unwrap();
        assert_eq!((width, height), (5, 2));
        assert_eq!(pixel(&rgba, width, 0, 0), &[40, 30, 20, 255]);
        assert_eq!(pixel(&rgba, width, 1, 0), &[0, 0, 0, 255]);
        assert_eq!(pixel(&rgba, width, 2, 1), &[0, 0, 0, 255]);
        assert_eq!(pixel(&rgba, width, 4, 1), &[70, 60, 50, 255]);
    }

    #[test]
    fn malformed_buffers_and_excessive_export_sizes_fail_without_indexing() {
        let mut desktop = desktop(PixelRect {
            left: -10,
            top: -10,
            width: 20,
            height: 20,
        });
        desktop.monitors[0].bgra.pop();
        let mut editor = Editor::new();
        editor.selection = Some(desktop.bounds);
        assert!(export_rgba(&desktop, &editor).is_err());
        assert!(pixel_length(u32::MAX, u32::MAX).is_err());
        assert!(pixel_length(0, 10).is_err());
    }

    #[test]
    fn native_export_translates_and_clips_annotation_geometry() {
        let desktop = desktop(PixelRect {
            left: -140,
            top: -60,
            width: 160,
            height: 100,
        });
        let mut editor = Editor::new();
        editor.selection = Some(PixelRect {
            left: -120,
            top: -50,
            width: 80,
            height: 60,
        });
        editor
            .commit(annotation(
                Tool::Rectangle,
                Point {
                    x: -115.0,
                    y: -45.0,
                },
                Point { x: -75.0, y: -10.0 },
                [240, 30, 60, 255],
            ))
            .unwrap();
        editor
            .commit(annotation(
                Tool::Pen,
                Point {
                    x: -135.0,
                    y: -35.0,
                },
                Point { x: -60.0, y: -35.0 },
                [30, 90, 240, 255],
            ))
            .unwrap();
        let (width, height, rgba) = export_rgba(&desktop, &editor).unwrap();
        assert_eq!((width, height), (80, 60));
        let red = pixel(&rgba, width, 5, 30);
        assert!(red[0] > 220 && red[1] < 60 && red[2] < 90, "{red:?}");
        let blue = pixel(&rgba, width, 10, 15);
        assert!(blue[0] < 60 && blue[1] < 120 && blue[2] > 210, "{blue:?}");
        assert_eq!(pixel(&rgba, width, 70, 55), &[155, 65, 90, 255]);
    }

    #[test]
    fn native_mosaic_crop_matches_preview_pixels_exactly() {
        let desktop = desktop(PixelRect {
            left: -30,
            top: -20,
            width: 80,
            height: 60,
        });
        let mut editor = Editor::new();
        editor.selection = Some(desktop.bounds);
        editor
            .commit(annotation(
                Tool::Mosaic,
                desktop.bounds.origin(),
                Point { x: 50.0, y: 40.0 },
                [0, 0, 0, 255],
            ))
            .unwrap();
        let (full_width, _, full) = export_rgba(&desktop, &editor).unwrap();
        editor.selection = Some(PixelRect {
            left: -13,
            top: -9,
            width: 45,
            height: 30,
        });
        let (width, height, cropped) = export_rgba(&desktop, &editor).unwrap();
        for y in 0..height {
            for x in 0..width {
                assert_eq!(
                    pixel(&cropped, width, x, y),
                    pixel(&full, full_width, x + 17, y + 11)
                );
            }
        }
    }

    #[test]
    fn native_scene_uploads_only_the_intersecting_viewport() {
        let _resources = SharedScope;
        let bounds = PixelRect {
            left: -60,
            top: -40,
            width: 120,
            height: 80,
        };
        let mut desktop = desktop(bounds);
        desktop.monitors.push(monitor(PixelRect {
            left: 60,
            top: -40,
            width: 120,
            height: 80,
        }));
        let viewport = PixelRect {
            left: -50,
            top: -30,
            width: 20,
            height: 15,
        };
        render_offscreen(20, 15, |target| {
            let canvas = Canvas::new(target, shared().unwrap(), 1.0).unwrap();
            let scene = Scene::new(&canvas, &desktop, viewport).unwrap();
            assert_eq!(scene.tiles.len(), 1);
            assert_eq!(scene.tiles[0].bounds, viewport);
            let frame = canvas.begin_draw();
            scene.paint_background(&canvas, viewport.origin());
            frame.finish().map_err(|error| error.to_string())
        })
        .unwrap();
    }

    #[test]
    fn optional_native_document_preview() {
        let Some(directory) = std::env::var_os("NCD_SCREENSHOT_SHOTS_DIR") else {
            return;
        };
        let mut desktop = desktop(PixelRect {
            left: -100,
            top: -50,
            width: 960,
            height: 600,
        });
        for y in 0..600usize {
            for x in 0..960usize {
                let color = if x < 185 {
                    [246, 238, 243, 255]
                } else if (210..825).contains(&x) && (115..420).contains(&y) {
                    if (425..715).contains(&x) && (170..220).contains(&y) {
                        let shade = if (x / 5 + y / 3) % 3 == 0 { 80 } else { 230 };
                        [shade, shade, shade, 255]
                    } else {
                        [255, 255, 255, 255]
                    }
                } else {
                    [248, 247, 249, 255]
                };
                let at = (y * 960 + x) * 4;
                desktop.monitors[0].bgra[at..at + 4].copy_from_slice(&color);
            }
        }
        let point = |x: f32, y: f32| Point {
            x: x - 100.0,
            y: y - 50.0,
        };
        let red = [242, 79, 92, 255];
        let blue = [59, 130, 246, 255];
        let mut editor = Editor::new();
        editor.selection = Some(desktop.bounds);
        editor
            .commit(annotation(
                Tool::Rectangle,
                point(210.0, 115.0),
                point(825.0, 420.0),
                red,
            ))
            .unwrap();
        editor
            .commit(annotation(
                Tool::Ellipse,
                point(245.0, 170.0),
                point(405.0, 225.0),
                blue,
            ))
            .unwrap();
        editor
            .commit(annotation(
                Tool::Arrow,
                point(650.0, 75.0),
                point(660.0, 145.0),
                red,
            ))
            .unwrap();
        editor
            .commit(annotation(
                Tool::Mosaic,
                point(425.0, 170.0),
                point(715.0, 220.0),
                red,
            ))
            .unwrap();
        let mut text = Annotation::new(Tool::Text, point(245.0, 275.0), red, 3.0);
        text.text = "截图保留在当前草稿\n中文输入与多屏物理像素".into();
        editor.commit(text).unwrap();
        editor
            .commit(Annotation::new(Tool::Number, point(230.0, 150.0), red, 3.0))
            .unwrap();
        let mut pen = Annotation::new(Tool::Pen, point(250.0, 365.0), blue, 4.0);
        pen.points.extend([
            point(270.0, 354.0),
            point(290.0, 369.0),
            point(315.0, 355.0),
            point(350.0, 363.0),
            point(405.0, 360.0),
        ]);
        editor.commit(pen).unwrap();
        let (width, height, rgba) = export_rgba(&desktop, &editor).unwrap();
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("screenshot-document-native.png");
        image::save_buffer(&path, &rgba, width, height, image::ColorType::Rgba8).unwrap();
        println!("Native screenshot document preview: {}", path.display());
    }
}
