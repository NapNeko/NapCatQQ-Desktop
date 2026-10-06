// Direct2D / DirectWrite 绘制层：工厂、字体、文字排版和画布原语。
//
// 坐标一律是 DIP（CSS 像素），渲染目标的 DPI 设成 96 × 缩放，D2D 自己换算到设备像素。
// 1px 的线和描边要落在设备像素上才不发虚，所以画线的地方先按缩放取整。

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
    D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2, D2D1_LINE_JOIN_ROUND,
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES1,
    D2D1_STROKE_TRANSFORM_TYPE_NORMAL, D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
    D2D1CreateFactory, ID2D1Bitmap, ID2D1Factory1, ID2D1PathGeometry, ID2D1PathGeometry1, ID2D1RenderTarget,
    ID2D1SolidColorBrush, ID2D1StrokeStyle1,
};
use windows::Win32::Graphics::Direct2D::Common::D2D1_BEZIER_SEGMENT;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_CONTAINER_TYPE_WOFF2, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_UNICODE_RANGE,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory5, IDWriteFontCollection1,
    IDWriteFontFallback, IDWriteTextFormat, IDWriteTextFormat1, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{HSTRING, Interface, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use super::icons::{self, Icon};
use super::theme::Rgba;

const INTER: &[u8] = include_bytes!("../../assets/fonts/inter-latin-wght-normal.woff2");
const MONO: &[u8] = include_bytes!("../../assets/fonts/jetbrains-mono-latin-wght-normal.woff2");

/// 和 tokens.css 的 --font-cjk-sans 同序；没装的跳过，最后落到系统回退。
const CJK_STACK: [&str; 5] = ["HarmonyOS Sans SC", "MiSans VF", "PingFang SC", "Microsoft YaHei UI", "Microsoft YaHei"];

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
        Self::new(self.x + dx, self.y + dy, self.w - 2.0 * dx, self.h - 2.0 * dy)
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.right(), bottom: self.bottom() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Family {
    Sans,
    Mono,
}

/// 一段文字的样式：字号、字重、行高都按 CSS 的写法给。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub family: Family,
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
}

impl TextStyle {
    pub const fn sans(size: f32, weight: u16, line_height: f32) -> Self {
        Self { family: Family::Sans, size, weight, line_height }
    }
    pub const fn mono(size: f32, weight: u16, line_height: f32) -> Self {
        Self { family: Family::Mono, size, weight, line_height }
    }
}

/// 排好的一行字。宽度是实际字宽（截断后不超过给定的最大宽度），高度就是行高。
pub struct TextBox {
    pub layout: IDWriteTextLayout,
    pub width: f32,
    pub height: f32,
}

struct FontFace {
    family: HSTRING,
    /// 以字号为 1 的上伸、下伸，算 CSS 行盒里的基线位置用
    ascent: f32,
    descent: f32,
}

/// 字体和排版，和渲染目标无关，整条线程共用一份。
pub struct TextSystem {
    dwrite: IDWriteFactory5,
    collection: IDWriteFontCollection1,
    fallback: IDWriteFontFallback,
    sans: FontFace,
    mono: FontFace,
    formats: RefCell<HashMap<(u8, u32, u16, u32), IDWriteTextFormat>>,
}

impl TextSystem {
    fn new() -> Result<Self> {
        // SAFETY: 以下都是 DirectWrite 的 COM 调用，参数是本函数里活着的局部值。
        unsafe {
            let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loader = dwrite.CreateInMemoryFontFileLoader()?;
            dwrite.RegisterFontFileLoader(&loader)?;
            let builder = dwrite.CreateFontSetBuilder()?;
            for woff2 in [INTER, MONO] {
                // DirectWrite 不直接认 woff2，先解成 OpenType；owner 传空时加载器自己拷一份数据
                let stream = dwrite.UnpackFontFile(DWRITE_CONTAINER_TYPE_WOFF2, woff2.as_ptr().cast(), woff2.len() as u32)?;
                let size = stream.GetFileSize()?;
                let mut start = std::ptr::null_mut();
                let mut context = std::ptr::null_mut();
                stream.ReadFileFragment(&mut start, 0, size, &mut context)?;
                let file = loader.CreateInMemoryFontFileReference(&dwrite, start, size as u32, None);
                stream.ReleaseFileFragment(context);
                builder.AddFontFile(&file?)?;
            }
            let set = builder.CreateFontSet()?;
            let collection = dwrite.CreateFontCollectionFromFontSet(&set)?;
            let sans = face(&collection, &["Inter", "Inter Variable"])?;
            let mono = face(&collection, &["JetBrains Mono", "JetBrains Mono Variable"])?;

            let builder = dwrite.CreateFontFallbackBuilder()?;
            let names: Vec<HSTRING> = CJK_STACK.iter().map(|n| HSTRING::from(*n)).collect();
            let pointers: Vec<*const u16> = names.iter().map(|n| n.as_ptr()).collect();
            let all = [DWRITE_UNICODE_RANGE { first: 0, last: 0x10FFFF }];
            builder.AddMapping(&all, &pointers, None, None, None, 1.0)?;
            builder.AddMappings(&dwrite.GetSystemFontFallback()?)?;
            let fallback = builder.CreateFontFallback()?;
            Ok(Self { dwrite, collection, fallback, sans, mono, formats: RefCell::new(HashMap::new()) })
        }
    }

    fn format(&self, style: TextStyle) -> Result<IDWriteTextFormat> {
        let key = (style.family as u8, style.size.to_bits(), style.weight, style.line_height.to_bits());
        if let Some(format) = self.formats.borrow().get(&key) {
            return Ok(format.clone());
        }
        let face = match style.family {
            Family::Sans => &self.sans,
            Family::Mono => &self.mono,
        };
        // SAFETY: DirectWrite COM 调用，参数都是活着的局部值或 self 的字段。
        let format = unsafe {
            let format = self.dwrite.CreateTextFormat(
                &face.family,
                &self.collection,
                DWRITE_FONT_WEIGHT(i32::from(style.weight)),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                w!("zh-CN"),
            )?;
            format.cast::<IDWriteTextFormat1>()?.SetFontFallback(&self.fallback)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            // CSS 行盒：内容区（上伸 + 下伸）在行高里上下居中，基线在内容区顶端往下一个上伸
            let content = (face.ascent + face.descent) * style.size;
            let baseline = (style.line_height - content) / 2.0 + face.ascent * style.size;
            format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, style.line_height, baseline)?;
            let sign = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
            let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
            format.SetTrimming(&trimming, &sign)?;
            format
        };
        self.formats.borrow_mut().insert(key, format.clone());
        Ok(format)
    }

    /// 排一行字，超过 `max_width` 时尾部省略号截断。
    pub fn layout(&self, text: &str, style: TextStyle, max_width: f32) -> Result<TextBox> {
        let format = self.format(style)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: DirectWrite COM 调用，wide 活到调用结束。
        unsafe {
            let layout = self.dwrite.CreateTextLayout(&wide, &format, max_width.max(0.0), style.line_height)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(TextBox { layout, width: metrics.widthIncludingTrailingWhitespace.min(max_width), height: style.line_height })
        }
    }
}

fn face(collection: &IDWriteFontCollection1, names: &[&str]) -> Result<FontFace> {
    // SAFETY: DirectWrite COM 调用，out 参数是局部变量。
    unsafe {
        for name in names {
            let family = HSTRING::from(*name);
            let mut index = 0;
            let mut exists = windows::core::BOOL::default();
            collection.FindFamilyName(&family, &mut index, &mut exists)?;
            if !exists.as_bool() {
                continue;
            }
            let font = collection.GetFontFamily(index)?.GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )?;
            let mut metrics = DWRITE_FONT_METRICS::default();
            font.GetMetrics(&mut metrics);
            let em = f32::from(metrics.designUnitsPerEm);
            return Ok(FontFace { family, ascent: f32::from(metrics.ascent) / em, descent: f32::from(metrics.descent) / em });
        }
    }
    Err(windows::core::Error::new(windows::Win32::Foundation::E_FAIL, format!("内置字体缺少 {names:?}")))
}

/// 和渲染目标无关的资源：D2D 工厂、图标几何、圆头描边样式、文字系统。整条 UI 线程一份。
pub struct Shared {
    pub d2d: ID2D1Factory1,
    pub text: TextSystem,
    round_stroke: ID2D1StrokeStyle1,
    icons: RefCell<HashMap<Icon, ID2D1PathGeometry1>>,
}

thread_local! {
    static SHARED: RefCell<Option<std::rc::Rc<Shared>>> = const { RefCell::new(None) };
}

/// 取本线程的共享资源，第一次用时创建。
pub fn shared() -> Result<std::rc::Rc<Shared>> {
    SHARED.with(|slot| {
        if let Some(shared) = slot.borrow().as_ref() {
            return Ok(shared.clone());
        }
        // SAFETY: D2D 工厂和描边样式的创建，参数是局部值。
        let shared = unsafe {
            let d2d: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let props = D2D1_STROKE_STYLE_PROPERTIES1 {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
                transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
            };
            let round_stroke = d2d.CreateStrokeStyle(&props, None)?;
            std::rc::Rc::new(Shared { d2d, text: TextSystem::new()?, round_stroke, icons: RefCell::new(HashMap::new()) })
        };
        *slot.borrow_mut() = Some(shared.clone());
        Ok(shared)
    })
}

impl Shared {
    fn icon(&self, icon: Icon) -> Result<ID2D1PathGeometry1> {
        if let Some(geometry) = self.icons.borrow().get(&icon) {
            return Ok(geometry.clone());
        }
        let geometry = build_path(&self.d2d, icons::paths(icon))?;
        self.icons.borrow_mut().insert(icon, geometry.clone());
        Ok(geometry)
    }
}

fn build_path(factory: &ID2D1Factory1, paths: &[&str]) -> Result<ID2D1PathGeometry1> {
    // SAFETY: 几何构建的 COM 调用，sink 在本函数里打开、关闭。
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        for d in paths {
            let mut open = false;
            for segment in icons::parse(d) {
                match segment {
                    icons::Seg::Move(p) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        // 描边和填充共用一份几何：图形都标成可填充，只描边的图标不会去填它
                        sink.BeginFigure(v(p), D2D1_FIGURE_BEGIN_FILLED);
                        open = true;
                    }
                    icons::Seg::Line(p) => sink.AddLine(v(p)),
                    icons::Seg::Cubic(a, b, c) => sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: v(a), point2: v(b), point3: v(c) }),
                    icons::Seg::Arc { to, r, rotation, large, sweep } => sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: v(to),
                        size: D2D_SIZE_F { width: r.0, height: r.1 },
                        rotationAngle: rotation,
                        sweepDirection: if sweep { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                        arcSize: if large { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                    }),
                    icons::Seg::Close => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            open = false;
                        }
                    }
                }
            }
            if open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
        }
        sink.Close()?;
        Ok(geometry)
    }
}

fn v(p: (f32, f32)) -> Vector2 {
    Vector2 { X: p.0, Y: p.1 }
}

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}

/// 解码好的图片（头像），straight alpha 的 RGBA。
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 绑定在某个渲染目标上的画布。图片位图跟着渲染目标走，换目标时整个丢掉重建。
pub struct Canvas {
    rt: ID2D1RenderTarget,
    shared: std::rc::Rc<Shared>,
    brush: ID2D1SolidColorBrush,
    bitmaps: RefCell<HashMap<usize, (std::rc::Weak<Image>, ID2D1Bitmap)>>,
    pub scale: f32,
}

impl Canvas {
    pub fn new(rt: ID2D1RenderTarget, shared: std::rc::Rc<Shared>, scale: f32) -> Result<Self> {
        // SAFETY: D2D COM 调用，rt 由调用方保证有效。
        let brush = unsafe {
            rt.SetDpi(96.0 * scale, 96.0 * scale);
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.CreateSolidColorBrush(&color(Rgba::TRANSPARENT), None)?
        };
        Ok(Self { rt, shared, brush, bitmaps: RefCell::new(HashMap::new()), scale })
    }

    pub fn target(&self) -> &ID2D1RenderTarget {
        &self.rt
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        // SAFETY: 设置渲染目标 DPI，不涉及外部内存。
        unsafe { self.rt.SetDpi(96.0 * scale, 96.0 * scale) };
    }

    /// 坐标按设备像素取整，画 1px 线和贴边的块时用
    pub fn snap(&self, value: f32) -> f32 {
        (value * self.scale).round() / self.scale
    }

    fn snap_rect(&self, r: Rect) -> Rect {
        let x = self.snap(r.x);
        let y = self.snap(r.y);
        Rect::new(x, y, self.snap(r.right()) - x, self.snap(r.bottom()) - y)
    }

    fn paint(&self, c: Rgba) -> &ID2D1SolidColorBrush {
        // SAFETY: 改纯色画刷的颜色，参数是栈上的值。
        unsafe { self.brush.SetColor(&color(c)) };
        &self.brush
    }

    pub fn clear(&self, c: Rgba) {
        // SAFETY: 清屏，参数是栈上的值。
        unsafe { self.rt.Clear(Some(&color(c))) };
    }

    pub fn fill(&self, r: Rect, c: Rgba) {
        if c.a <= 0.0 {
            return;
        }
        let r = self.snap_rect(r);
        // SAFETY: D2D 绘制调用，参数是栈上的值。
        unsafe { self.rt.FillRectangle(&r.d2d(), self.paint(c)) };
    }

    pub fn fill_round(&self, r: Rect, radius: f32, c: Rgba) {
        if c.a <= 0.0 {
            return;
        }
        let r = self.snap_rect(r);
        let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        // SAFETY: D2D 绘制调用，参数是栈上的值。
        unsafe { self.rt.FillRoundedRectangle(&rr, self.paint(c)) };
    }

    /// CSS 的 ring-1：贴着盒子外沿画一圈 1px，不压到盒子里面。
    pub fn ring(&self, r: Rect, radius: f32, c: Rgba) {
        if c.a <= 0.0 {
            return;
        }
        let r = self.snap_rect(r);
        let px = (self.scale.floor().max(1.0)) / self.scale;
        let half = px / 2.0;
        let outer = Rect::new(r.x - half, r.y - half, r.w + px, r.h + px);
        let radius = radius.min(r.w / 2.0).min(r.h / 2.0) + half;
        let rr = D2D1_ROUNDED_RECT { rect: outer.d2d(), radiusX: radius, radiusY: radius };
        // SAFETY: D2D 绘制调用，参数是栈上的值。
        unsafe { self.rt.DrawRoundedRectangle(&rr, self.paint(c), px, None) };
    }

    /// 1px 横线（border-top），按设备像素对齐。
    pub fn hline(&self, x: f32, right: f32, y: f32, c: Rgba) {
        let px = (self.scale.floor().max(1.0)) / self.scale;
        let y = self.snap(y);
        self.fill(Rect::new(x, y, right - x, px), c);
    }

    pub fn circle(&self, cx: f32, cy: f32, radius: f32, c: Rgba) {
        if c.a <= 0.0 {
            return;
        }
        let e = D2D1_ELLIPSE { point: v((cx, cy)), radiusX: radius, radiusY: radius };
        // SAFETY: D2D 绘制调用，参数是栈上的值。
        unsafe { self.rt.FillEllipse(&e, self.paint(c)) };
    }

    pub fn text(&self, text: &TextBox, x: f32, y: f32, c: Rgba) {
        // SAFETY: D2D 绘制调用，排版对象活到调用结束。
        unsafe {
            self.rt.DrawTextLayout(v((x, y)), &text.layout, self.paint(c), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        }
    }

    /// lucide 图标：24 单位的画板缩放到 `size`，描边宽度按画板单位给（和 lucide 的 strokeWidth 一致）。
    pub fn icon(&self, icon: Icon, x: f32, y: f32, size: f32, stroke: f32, c: Rgba, filled: bool) {
        let Ok(geometry) = self.shared.icon(icon) else { return };
        let k = size / 24.0;
        let transform = Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: x, M32: y };
        // SAFETY: D2D 绘制调用，变换在调用后复位。
        unsafe {
            self.rt.SetTransform(&transform);
            let brush = self.paint(c);
            if filled {
                self.rt.FillGeometry(&geometry, brush, None);
            }
            self.rt.DrawGeometry(&geometry, brush, stroke, &self.shared.round_stroke);
            self.rt.SetTransform(&Matrix3x2::identity());
        }
    }

    /// 旋转着画（加载圈）：绕图标中心转 `degrees`。
    pub fn icon_rotated(&self, icon: Icon, x: f32, y: f32, size: f32, stroke: f32, c: Rgba, degrees: f32) {
        let Ok(geometry) = self.shared.icon(icon) else { return };
        let k = size / 24.0;
        let (s, cos) = degrees.to_radians().sin_cos();
        let (cx, cy) = (x + size / 2.0, y + size / 2.0);
        // 先缩放到 size、平移到中心为原点，再旋转，最后移到目标中心
        let m = Matrix3x2 {
            M11: k * cos,
            M12: k * s,
            M21: -k * s,
            M22: k * cos,
            M31: cx - (12.0 * k * cos - 12.0 * k * s),
            M32: cy - (12.0 * k * s + 12.0 * k * cos),
        };
        // SAFETY: D2D 绘制调用，变换在调用后复位。
        unsafe {
            self.rt.SetTransform(&m);
            self.rt.DrawGeometry(&geometry, self.paint(c), stroke, &self.shared.round_stroke);
            self.rt.SetTransform(&Matrix3x2::identity());
        }
    }

    /// 左上到右下的两色渐变圆角块（头像加载不到时的底色）。
    pub fn gradient_round(&self, r: Rect, radius: f32, from: Rgba, to: Rgba) {
        let r = self.snap_rect(r);
        let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: color(from) }, D2D1_GRADIENT_STOP { position: 1.0, color: color(to) }];
        // SAFETY: D2D 资源创建与绘制，参数是栈上的值。
        unsafe {
            let Ok(collection) = self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: v((r.x, r.y)), endPoint: v((r.right(), r.bottom())) };
            let Ok(brush) = self.rt.CreateLinearGradientBrush(&props, None, &collection) else { return };
            let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
            self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius }, &brush);
        }
    }

    /// 图片铺满 `r` 并裁成圆角（object-fit: cover，按短边缩放后居中裁）。
    pub fn image_round(&self, image: &std::rc::Rc<Image>, r: Rect, radius: f32) {
        let Some(bitmap) = self.bitmap(image) else { return };
        let r = self.snap_rect(r);
        let (iw, ih) = (image.width as f32, image.height as f32);
        let k = (r.w / iw).max(r.h / ih);
        let (dx, dy) = (r.x + (r.w - iw * k) / 2.0, r.y + (r.h - ih * k) / 2.0);
        // SAFETY: D2D 资源创建与绘制；位图画刷的变换把图片像素映射到目标矩形。
        unsafe {
            use windows::Win32::Graphics::Direct2D::{D2D1_BITMAP_BRUSH_PROPERTIES, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR};
            let props = D2D1_BITMAP_BRUSH_PROPERTIES {
                extendModeX: D2D1_EXTEND_MODE_CLAMP,
                extendModeY: D2D1_EXTEND_MODE_CLAMP,
                interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
            };
            let Ok(brush) = self.rt.CreateBitmapBrush(&bitmap, Some(&props), None) else { return };
            // 位图按 96 DPI 建的，1 位图像素 = 1 DIP，所以直接按 k 缩放
            brush.SetTransform(&Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: dx, M32: dy });
            let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
            self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius }, &brush);
        }
    }

    fn bitmap(&self, image: &std::rc::Rc<Image>) -> Option<ID2D1Bitmap> {
        let key = std::rc::Rc::as_ptr(image) as usize;
        if let Some((weak, bitmap)) = self.bitmaps.borrow().get(&key)
            && weak.upgrade().is_some_and(|alive| std::rc::Rc::ptr_eq(&alive, image))
        {
            return Some(bitmap.clone());
        }
        // D2D 要预乘的 BGRA
        let mut bgra = Vec::with_capacity(image.rgba.len());
        for px in image.rgba.chunks_exact(4) {
            let a = u16::from(px[3]);
            let mul = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            bgra.extend_from_slice(&[mul(px[2]), mul(px[1]), mul(px[0]), px[3]]);
        }
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        // SAFETY: bgra 活到调用结束，pitch 与宽度一致。
        let bitmap = unsafe {
            self.rt
                .CreateBitmap(D2D_SIZE_U { width: image.width, height: image.height }, Some(bgra.as_ptr().cast()), image.width * 4, &props)
                .ok()?
        };
        let mut cache = self.bitmaps.borrow_mut();
        cache.retain(|_, (weak, _)| weak.strong_count() > 0);
        cache.insert(key, (std::rc::Rc::downgrade(image), bitmap.clone()));
        Some(bitmap)
    }

    pub fn clip(&self, r: Rect) {
        // SAFETY: D2D 裁剪栈，和 unclip 成对调用。
        unsafe { self.rt.PushAxisAlignedClip(&self.snap_rect(r).d2d(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
    }

    pub fn unclip(&self) {
        // SAFETY: 弹出 clip 压进去的裁剪。
        unsafe { self.rt.PopAxisAlignedClip() };
    }
}
