//! GTK 绘制回调中的 Cairo / Pango 后端；坐标与 Windows 面板共用同一套 DIP 布局。

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::rc::{Rc, Weak};

use gtk::{cairo, pango};

use super::icons::{self, Icon, Seg};
use super::theme::Rgba;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(
            self.x + dx,
            self.y + dy,
            self.w - 2.0 * dx,
            self.h - 2.0 * dy,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Family {
    Sans,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub family: Family,
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
}

impl TextStyle {
    pub const fn sans(size: f32, weight: u16, line_height: f32) -> Self {
        Self {
            family: Family::Sans,
            size,
            weight,
            line_height,
        }
    }

    pub const fn mono(size: f32, weight: u16, line_height: f32) -> Self {
        Self {
            family: Family::Mono,
            size,
            weight,
            line_height,
        }
    }
}

/// 排版只在布局变化时创建，绘制和 Clone 均复用 PangoLayout。
#[derive(Clone)]
pub struct TextBox {
    layout: pango::Layout,
    y_offset: f32,
    pub width: f32,
    pub height: f32,
}

pub struct TextSystem {
    context: pango::Context,
}

impl TextSystem {
    pub fn layout(&self, text: &str, style: TextStyle, max_width: f32) -> Result<TextBox, String> {
        let layout = pango::Layout::new(&self.context);
        let mut font = pango::FontDescription::new();
        font.set_family(match style.family {
            Family::Sans => "Inter, Noto Sans CJK SC, sans-serif",
            Family::Mono => "JetBrains Mono, monospace",
        });
        font.set_absolute_size(f64::from(style.size) * f64::from(pango::SCALE));
        font.set_weight(match style.weight {
            0..=449 => pango::Weight::Normal,
            450..=549 => pango::Weight::Medium,
            550..=649 => pango::Weight::Semibold,
            650..=749 => pango::Weight::Bold,
            _ => pango::Weight::Heavy,
        });
        layout.set_font_description(Some(&font));
        layout.set_single_paragraph_mode(true);
        layout.set_ellipsize(pango::EllipsizeMode::End);
        layout.set_width((max_width.max(0.0) * pango::SCALE as f32).round() as i32);
        layout.set_height(-1);
        layout.set_text(text);
        let (_, logical) = layout.extents();
        let unit = pango::SCALE as f32;
        let actual_height = logical.height() as f32 / unit;
        Ok(TextBox {
            layout,
            y_offset: (style.line_height - actual_height) / 2.0 - logical.y() as f32 / unit,
            width: (logical.width() as f32 / unit).min(max_width.max(0.0)),
            height: style.line_height,
        })
    }
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

type BitmapCache = HashMap<(usize, u32, u32), (Weak<Image>, cairo::ImageSurface)>;

pub struct Shared {
    pub text: TextSystem,
    geometry: cairo::Context,
    icons: RefCell<HashMap<Icon, Rc<cairo::Path>>>,
    bitmaps: RefCell<BitmapCache>,
}

thread_local! {
    // 窗口持有强引用；最后一个面板释放后不保留字体与位图缓存。
    static SHARED: RefCell<Weak<Shared>> = const { RefCell::new(Weak::new()) };
}

pub fn release_shared() {
    SHARED.with(|slot| *slot.borrow_mut() = Weak::new());
}

pub fn shared() -> Result<Rc<Shared>, String> {
    SHARED.with(|slot| {
        if let Some(shared) = slot.borrow().upgrade() {
            return Ok(shared);
        }
        let surface = cairo::RecordingSurface::create(cairo::Content::ColorAlpha, None)
            .map_err(|error| error.to_string())?;
        let geometry = cairo::Context::new(surface).map_err(|error| error.to_string())?;
        let mut options = cairo::FontOptions::new().map_err(|error| error.to_string())?;
        // 逻辑排版不随设备像素取整，GTK DPI 和界面倍率由绘制回调的变换承担。
        options.set_hint_metrics(cairo::HintMetrics::Off);
        // 透明表面和分数倍率不能保留 LCD 子像素彩边。
        options.set_antialias(cairo::Antialias::Gray);
        geometry.set_font_options(&options);
        let shared = Rc::new(Shared {
            text: TextSystem {
                context: pangocairo::functions::create_context(&geometry),
            },
            geometry,
            icons: RefCell::new(HashMap::new()),
            bitmaps: RefCell::new(HashMap::new()),
        });
        *slot.borrow_mut() = Rc::downgrade(&shared);
        Ok(shared)
    })
}

impl Shared {
    fn icon(&self, icon: Icon) -> Result<Rc<cairo::Path>, String> {
        if let Some(path) = self.icons.borrow().get(&icon) {
            return Ok(Rc::clone(path));
        }
        self.geometry.new_path();
        for path in icons::paths(icon) {
            append_svg(&self.geometry, &icons::parse(path));
        }
        let path = Rc::new(
            self.geometry
                .copy_path()
                .map_err(|error| error.to_string())?,
        );
        self.icons.borrow_mut().insert(icon, Rc::clone(&path));
        Ok(path)
    }

    fn bitmap(&self, image: &Rc<Image>, w: u32, h: u32) -> Result<cairo::ImageSurface, String> {
        let key = (Rc::as_ptr(image) as usize, w, h);
        if let Some((weak, surface)) = self.bitmaps.borrow().get(&key)
            && weak
                .upgrade()
                .is_some_and(|alive| Rc::ptr_eq(&alive, image))
        {
            return Ok(surface.clone());
        }
        let pixels =
            cover_resize(image, w, h).ok_or_else(|| "托盘头像像素尺寸不匹配".to_string())?;
        // Cairo ARGB32 是本机字节序的预乘像素，输入头像为 straight RGBA。
        let mut argb = Vec::with_capacity(pixels.len());
        for px in pixels.chunks_exact(4) {
            let &[r, g, b, a] = px else { continue };
            let mul = |c: u8| (u32::from(c) * u32::from(a) + 127) / 255;
            let value = u32::from(a) << 24 | mul(r) << 16 | mul(g) << 8 | mul(b);
            argb.extend_from_slice(&value.to_ne_bytes());
        }
        let width = i32::try_from(w).map_err(|error| error.to_string())?;
        let height = i32::try_from(h).map_err(|error| error.to_string())?;
        let stride = cairo::Format::ARgb32
            .stride_for_width(w)
            .map_err(|error| error.to_string())?;
        let surface = cairo::ImageSurface::create_for_data(
            argb,
            cairo::Format::ARgb32,
            width,
            height,
            stride,
        )
        .map_err(|error| error.to_string())?;
        let mut cache = self.bitmaps.borrow_mut();
        cache.retain(|_, (weak, _)| weak.strong_count() > 0);
        cache.insert(key, (Rc::downgrade(image), surface.clone()));
        Ok(surface)
    }
}

/// GTK 已给 context 应用系统 DPI，窗口后端再应用界面倍率；scale 仅用于像素对齐。
pub struct Canvas {
    context: cairo::Context,
    shared: Rc<Shared>,
    pub scale: f32,
}

impl Canvas {
    pub fn new(context: cairo::Context, scale: f32) -> Result<Self, String> {
        context.status().map_err(|error| error.to_string())?;
        context.set_line_cap(cairo::LineCap::Round);
        context.set_line_join(cairo::LineJoin::Round);
        Ok(Self {
            context,
            shared: shared()?,
            scale,
        })
    }

    /// Cairo 的绘制错误会留在 context 上，由帧末统一报告，避免每个原语重复记录。
    pub fn finish(&self) -> Result<(), String> {
        self.context.status().map_err(|error| error.to_string())
    }

    pub fn translate(&self, x: f32, y: f32) -> StateGuard {
        let guard = StateGuard::new(&self.context);
        self.context.translate(f64::from(x), f64::from(y));
        guard
    }

    pub fn snap(&self, value: f32) -> f32 {
        (value * self.scale).round() / self.scale
    }

    fn snap_rect(&self, rect: Rect) -> Rect {
        let x = self.snap(rect.x);
        let y = self.snap(rect.y);
        Rect::new(
            x,
            y,
            self.snap(rect.right()) - x,
            self.snap(rect.bottom()) - y,
        )
    }

    fn color(&self, color: Rgba) {
        self.context.set_source_rgba(
            f64::from(color.r),
            f64::from(color.g),
            f64::from(color.b),
            f64::from(color.a),
        );
    }

    pub fn clear(&self, color: Rgba) {
        let _state = StateGuard::new(&self.context);
        self.context.set_operator(cairo::Operator::Source);
        self.color(color);
        let _ = self.context.paint();
    }

    pub fn fill(&self, rect: Rect, color: Rgba) {
        let rect = self.snap_rect(rect);
        self.context.new_path();
        rectangle(&self.context, rect);
        self.color(color);
        let _ = self.context.fill();
    }

    pub fn fill_round(&self, rect: Rect, radius: f32, color: Rgba) {
        rounded_path(&self.context, self.snap_rect(rect), radius);
        self.color(color);
        let _ = self.context.fill();
    }

    pub fn ring(&self, rect: Rect, radius: f32, color: Rgba) {
        let rect = self.snap_rect(rect);
        let px = self.scale.floor().max(1.0) / self.scale;
        let half = px / 2.0;
        rounded_path(
            &self.context,
            Rect::new(rect.x - half, rect.y - half, rect.w + px, rect.h + px),
            radius.min(rect.w / 2.0).min(rect.h / 2.0) + half,
        );
        self.context.set_line_width(f64::from(px));
        self.color(color);
        let _ = self.context.stroke();
    }

    pub fn hline(&self, x: f32, right: f32, y: f32, color: Rgba) {
        let px = self.scale.floor().max(1.0) / self.scale;
        self.fill(Rect::new(x, self.snap(y), right - x, px), color);
    }

    pub fn circle(&self, x: f32, y: f32, radius: f32, color: Rgba) {
        self.context.new_path();
        self.context
            .arc(f64::from(x), f64::from(y), f64::from(radius), 0.0, TAU);
        self.color(color);
        let _ = self.context.fill();
    }

    pub fn text(&self, text: &TextBox, x: f32, y: f32, color: Rgba) {
        self.context
            .move_to(f64::from(x), f64::from(y + text.y_offset));
        self.color(color);
        pangocairo::functions::show_layout(&self.context, &text.layout);
    }

    #[expect(clippy::too_many_arguments, reason = "与共用视图的绘制原语保持一致")]
    pub fn icon(
        &self,
        icon: Icon,
        x: f32,
        y: f32,
        size: f32,
        stroke: f32,
        color: Rgba,
        filled: bool,
    ) {
        let _state = self.translate(x, y);
        self.context
            .scale(f64::from(size / 24.0), f64::from(size / 24.0));
        self.draw_icon(icon, stroke, color, filled);
    }

    #[expect(clippy::too_many_arguments, reason = "与共用视图的绘制原语保持一致")]
    pub fn icon_rotated(
        &self,
        icon: Icon,
        x: f32,
        y: f32,
        size: f32,
        stroke: f32,
        color: Rgba,
        degrees: f32,
    ) {
        let _state = self.translate(x + size / 2.0, y + size / 2.0);
        self.context.rotate(f64::from(degrees).to_radians());
        self.context
            .scale(f64::from(size / 24.0), f64::from(size / 24.0));
        self.context.translate(-12.0, -12.0);
        self.draw_icon(icon, stroke, color, false);
    }

    fn draw_icon(&self, icon: Icon, stroke: f32, color: Rgba, filled: bool) {
        let Ok(path) = self.shared.icon(icon) else {
            return;
        };
        self.context.new_path();
        self.context.append_path(&path);
        self.color(color);
        self.context.set_line_width(f64::from(stroke));
        if filled {
            let _ = self.context.fill_preserve();
        }
        let _ = self.context.stroke();
    }

    pub fn gradient_round(&self, rect: Rect, radius: f32, from: Rgba, to: Rgba) {
        let rect = self.snap_rect(rect);
        let gradient = cairo::LinearGradient::new(
            f64::from(rect.x),
            f64::from(rect.y),
            f64::from(rect.right()),
            f64::from(rect.bottom()),
        );
        for (offset, color) in [(0.0, from), (1.0, to)] {
            gradient.add_color_stop_rgba(
                offset,
                f64::from(color.r),
                f64::from(color.g),
                f64::from(color.b),
                f64::from(color.a),
            );
        }
        rounded_path(&self.context, rect, radius);
        let _ = self.context.set_source(gradient);
        let _ = self.context.fill();
    }

    pub fn image_round(&self, image: &Rc<Image>, rect: Rect, radius: f32) {
        let rect = self.snap_rect(rect);
        let width = (rect.w * self.scale).round().max(1.0) as u32;
        let height = (rect.h * self.scale).round().max(1.0) as u32;
        let surface = match self.shared.bitmap(image, width, height) {
            Ok(surface) => surface,
            Err(error) => {
                tracing::warn!(%error, "托盘头像绘制失败");
                return;
            }
        };
        let _state = StateGuard::new(&self.context);
        rounded_path(&self.context, rect, radius);
        self.context.clip();
        self.context.translate(f64::from(rect.x), f64::from(rect.y));
        self.context.scale(
            f64::from(rect.w) / f64::from(width),
            f64::from(rect.h) / f64::from(height),
        );
        let _ = self.context.set_source_surface(surface, 0.0, 0.0);
        self.context.source().set_filter(cairo::Filter::Bilinear);
        let _ = self.context.paint();
    }

    pub fn clip(&self, rect: Rect) -> StateGuard {
        let guard = StateGuard::new(&self.context);
        self.context.new_path();
        rectangle(&self.context, self.snap_rect(rect));
        self.context.clip();
        guard
    }

    pub fn opacity(&self, bounds: Rect, opacity: f32) -> OpacityGuard {
        let state = self.clip(bounds);
        self.context.push_group();
        OpacityGuard {
            state,
            opacity: f64::from(opacity),
        }
    }
}

pub struct StateGuard {
    context: cairo::Context,
}

impl StateGuard {
    fn new(context: &cairo::Context) -> Self {
        let _ = context.save();
        Self {
            context: context.clone(),
        }
    }
}

impl Drop for StateGuard {
    fn drop(&mut self) {
        let _ = self.context.restore();
    }
}

pub struct OpacityGuard {
    state: StateGuard,
    opacity: f64,
}

impl Drop for OpacityGuard {
    fn drop(&mut self) {
        let context = &self.state.context;
        let _ = context.pop_group_to_source();
        let _ = context.paint_with_alpha(self.opacity);
    }
}

fn rectangle(context: &cairo::Context, rect: Rect) {
    context.rectangle(
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.w),
        f64::from(rect.h),
    );
}

fn rounded_path(context: &cairo::Context, rect: Rect, radius: f32) {
    let radius = f64::from(radius.max(0.0).min(rect.w / 2.0).min(rect.h / 2.0));
    let (left, top) = (f64::from(rect.x), f64::from(rect.y));
    let (right, bottom) = (f64::from(rect.right()), f64::from(rect.bottom()));
    context.new_path();
    context.arc(right - radius, top + radius, radius, -FRAC_PI_2, 0.0);
    context.arc(right - radius, bottom - radius, radius, 0.0, FRAC_PI_2);
    context.arc(left + radius, bottom - radius, radius, FRAC_PI_2, PI);
    context.arc(left + radius, top + radius, radius, PI, PI + FRAC_PI_2);
    context.close_path();
}

fn append_svg(context: &cairo::Context, segments: &[Seg]) {
    let mut current = (0.0, 0.0);
    let mut start = current;
    for segment in segments {
        match *segment {
            Seg::Move(to) => {
                context.move_to(f64::from(to.0), f64::from(to.1));
                current = to;
                start = to;
            }
            Seg::Line(to) => {
                context.line_to(f64::from(to.0), f64::from(to.1));
                current = to;
            }
            Seg::Cubic(first, second, to) => {
                context.curve_to(
                    f64::from(first.0),
                    f64::from(first.1),
                    f64::from(second.0),
                    f64::from(second.1),
                    f64::from(to.0),
                    f64::from(to.1),
                );
                current = to;
            }
            Seg::Arc {
                to,
                r,
                rotation,
                large,
                sweep,
            } => {
                svg_arc(context, current, to, r, rotation, large, sweep);
                current = to;
            }
            Seg::Close => {
                context.close_path();
                current = start;
            }
        }
    }
}

/// SVG 端点形式转椭圆圆心形式，按 SVG 2 附录 B.2 修正半径和扫角。
fn svg_arc(
    context: &cairo::Context,
    from: (f32, f32),
    to: (f32, f32),
    radii: (f32, f32),
    rotation: f32,
    large: bool,
    sweep: bool,
) {
    if from == to {
        return;
    }
    let (mut rx, mut ry) = (f64::from(radii.0.abs()), f64::from(radii.1.abs()));
    if rx == 0.0 || ry == 0.0 {
        context.line_to(f64::from(to.0), f64::from(to.1));
        return;
    }
    let phi = f64::from(rotation).to_radians();
    let (sin, cos) = phi.sin_cos();
    let (dx, dy) = (
        f64::from(from.0 - to.0) / 2.0,
        f64::from(from.1 - to.1) / 2.0,
    );
    let (x, y) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let correction = x * x / (rx * rx) + y * y / (ry * ry);
    if correction > 1.0 {
        rx *= correction.sqrt();
        ry *= correction.sqrt();
    }
    let (rx2, ry2) = (rx * rx, ry * ry);
    let denominator = rx2 * y * y + ry2 * x * x;
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let factor = sign * ((rx2 * ry2 - denominator).max(0.0) / denominator).sqrt();
    let (cx, cy) = (factor * rx * y / ry, -factor * ry * x / rx);
    let center = (
        cos * cx - sin * cy + f64::from(from.0 + to.0) / 2.0,
        sin * cx + cos * cy + f64::from(from.1 + to.1) / 2.0,
    );
    let (ux, uy) = ((x - cx) / rx, (y - cy) / ry);
    let (vx, vy) = ((-x - cx) / rx, (-y - cy) / ry);
    let first = uy.atan2(ux);
    let mut delta = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    if sweep && delta < 0.0 {
        delta += TAU;
    } else if !sweep && delta > 0.0 {
        delta -= TAU;
    }
    let _state = StateGuard::new(context);
    context.translate(center.0, center.1);
    context.rotate(phi);
    context.scale(rx, ry);
    if sweep {
        context.arc(0.0, 0.0, 1.0, first, first + delta);
    } else {
        context.arc_negative(0.0, 0.0, 1.0, first, first + delta);
    }
}

fn cover_resize(image: &Image, width: u32, height: u32) -> Option<Vec<u8>> {
    let source = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())?;
    let (iw, ih) = (u64::from(image.width), u64::from(image.height));
    if iw == 0 || ih == 0 {
        return None;
    }
    let (cw, ch) = if iw * u64::from(height) > ih * u64::from(width) {
        ((ih * u64::from(width) / u64::from(height)).max(1), ih)
    } else {
        (iw, (iw * u64::from(height) / u64::from(width)).max(1))
    };
    let cropped = image::imageops::crop_imm(
        &source,
        ((iw - cw) / 2) as u32,
        ((ih - ch) / 2) as u32,
        cw as u32,
        ch as u32,
    )
    .to_image();
    if cropped.width() == width && cropped.height() == height {
        return Some(cropped.into_raw());
    }
    Some(
        image::imageops::resize(
            &cropped,
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        )
        .into_raw(),
    )
}
