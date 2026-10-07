// Direct2D / DirectWrite 绘制层：坐标是 DIP，渲染目标 DPI = 96 × 缩放。
#![expect(
    unsafe_code,
    reason = "D2D/DWrite 的 COM 方法全是 unsafe fn，对外只给安全的 Canvas / TextSystem"
)]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Graphics::Direct2D::Common::D2D1_BEZIER_SEGMENT;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP,
    D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID,
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2, D2D1_LINE_JOIN_ROUND,
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES1,
    D2D1_STROKE_TRANSFORM_TYPE_NORMAL, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, D2D1CreateFactory, ID2D1Bitmap, ID2D1Factory1,
    ID2D1PathGeometry1, ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle1,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_CONTAINER_TYPE_WOFF2, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_TEXT_METRICS,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_UNICODE_RANGE,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory5, IDWriteFontCollection1,
    IDWriteFontFallback, IDWriteInMemoryFontFileLoader, IDWriteTextFormat, IDWriteTextFormat1,
    IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{HSTRING, Interface, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use super::icons::{self, Icon};
use super::sys::{ClipGuard, DrawGuard, TransformGuard};
use super::theme::Rgba;

const INTER: &[u8] = include_bytes!("../../assets/fonts/inter-latin-wght-normal.woff2");
const MONO: &[u8] = include_bytes!("../../assets/fonts/jetbrains-mono-latin-wght-normal.woff2");

/// 和 tokens.css 的 --font-cjk-sans 同序；没装的跳过，最后落到系统回退。
const CJK_STACK: [&str; 5] = [
    "HarmonyOS Sans SC",
    "MiSans VF",
    "PingFang SC",
    "Microsoft YaHei UI",
    "Microsoft YaHei",
];

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
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.x,
            top: self.y,
            right: self.right(),
            bottom: self.bottom(),
        }
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

/// 排好的一行字。宽度是实际字宽（截断后不超过给定的最大宽度），高度就是行高。
/// Clone 共享排版和字体注册。
#[derive(Clone)]
pub struct TextBox {
    layout: IDWriteTextLayout,
    pub width: f32,
    pub height: f32,
    // 排版可以比 TextSystem 活得更久，加载器必须等最后一个排版释放后再注销。
    _fonts: Rc<FontLoaderRegistration>,
}

struct FontLoaderRegistration {
    factory: IDWriteFactory5,
    loader: IDWriteInMemoryFontFileLoader,
}

impl FontLoaderRegistration {
    fn new(factory: IDWriteFactory5) -> Result<Self> {
        // SAFETY: factory 是当前 UI 线程持有的 COM 引用，loader 注册后立即交给本所有者。
        unsafe {
            let loader = factory.CreateInMemoryFontFileLoader()?;
            factory.RegisterFontFileLoader(&loader)?;
            Ok(Self { factory, loader })
        }
    }
}

impl Drop for FontLoaderRegistration {
    fn drop(&mut self) {
        // SAFETY: 所有字体集合、format 和 layout 已释放；factory 和 loader 仍由本对象持有。
        if let Err(error) = unsafe { self.factory.UnregisterFontFileLoader(&self.loader) } {
            tracing::warn!(target: "ncd_tauri::native_panel", %error, "注销内置字体加载器失败");
        }
    }
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
    // 字段按声明顺序释放，注册所有者必须排在引用字体的字段之后。
    fonts: Rc<FontLoaderRegistration>,
}

impl TextSystem {
    fn new() -> Result<Self> {
        // SAFETY: DirectWrite COM 调用，参数是本函数里活着的局部值。裸指针有三处：
        // UnpackFontFile 读的是 'static 的 include_bytes；ReadFileFragment 给的 start 只在
        // ReleaseFileFragment 之前用，CreateInMemoryFontFileReference 传 owner = None 让加载器拷走数据，
        // 中间没有 `?` 提前返回漏掉 Release；AddMapping 的 pointers 指向 names 里的 HSTRING，names 活到调用之后。
        unsafe {
            let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let fonts = Rc::new(FontLoaderRegistration::new(dwrite.clone())?);
            let loader = &fonts.loader;
            let builder = dwrite.CreateFontSetBuilder()?;
            for woff2 in [INTER, MONO] {
                // DirectWrite 不直接认 woff2，先解成 OpenType；owner 传空时加载器自己拷一份数据
                let stream = dwrite.UnpackFontFile(
                    DWRITE_CONTAINER_TYPE_WOFF2,
                    woff2.as_ptr().cast(),
                    woff2.len() as u32,
                )?;
                let size = stream.GetFileSize()?;
                let mut start = std::ptr::null_mut();
                let mut context = std::ptr::null_mut();
                stream.ReadFileFragment(&mut start, 0, size, &mut context)?;
                let file =
                    loader.CreateInMemoryFontFileReference(&dwrite, start, size as u32, None);
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
            let all = [DWRITE_UNICODE_RANGE {
                first: 0,
                last: 0x10FFFF,
            }];
            builder.AddMapping(&all, &pointers, None, None, None, 1.0)?;
            builder.AddMappings(&dwrite.GetSystemFontFallback()?)?;
            let fallback = builder.CreateFontFallback()?;
            Ok(Self {
                dwrite,
                collection,
                fallback,
                sans,
                mono,
                formats: RefCell::new(HashMap::new()),
                fonts,
            })
        }
    }

    fn format(&self, style: TextStyle) -> Result<IDWriteTextFormat> {
        let key = (
            style.family as u8,
            style.size.to_bits(),
            style.weight,
            style.line_height.to_bits(),
        );
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
            format
                .cast::<IDWriteTextFormat1>()?
                .SetFontFallback(&self.fallback)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            // CSS 行盒：内容区（上伸 + 下伸）在行高里上下居中，基线在内容区顶端往下一个上伸
            let content = (face.ascent + face.descent) * style.size;
            let baseline = (style.line_height - content) / 2.0 + face.ascent * style.size;
            format.SetLineSpacing(
                DWRITE_LINE_SPACING_METHOD_UNIFORM,
                style.line_height,
                baseline,
            )?;
            let sign = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
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
            let layout = self.dwrite.CreateTextLayout(
                &wide,
                &format,
                max_width.max(0.0),
                style.line_height,
            )?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(TextBox {
                layout,
                width: metrics.widthIncludingTrailingWhitespace.min(max_width),
                height: style.line_height,
                _fonts: Rc::clone(&self.fonts),
            })
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
            return Ok(FontFace {
                family,
                ascent: f32::from(metrics.ascent) / em,
                descent: f32::from(metrics.descent) / em,
            });
        }
    }
    Err(windows::core::Error::new(
        windows::Win32::Foundation::E_FAIL,
        format!("内置字体缺少 {names:?}"),
    ))
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

/// 面板都销毁后放掉 D2D / DirectWrite（字体回退、字形缓存占好几 MB），下次用时重建。
/// 还有画布持着 Rc 时只是摘掉本线程这份引用，画布释放时一起走。
pub fn release_shared() {
    let shared = SHARED.with(|slot| slot.borrow_mut().take());
    drop(shared);
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
            std::rc::Rc::new(Shared {
                d2d,
                text: TextSystem::new()?,
                round_stroke,
                icons: RefCell::new(HashMap::new()),
            })
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
                    icons::Seg::Cubic(a, b, c) => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: v(a),
                        point2: v(b),
                        point3: v(c),
                    }),
                    icons::Seg::Arc {
                        to,
                        r,
                        rotation,
                        large,
                        sweep,
                    } => sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: v(to),
                        size: D2D_SIZE_F {
                            width: r.0,
                            height: r.1,
                        },
                        rotationAngle: rotation,
                        sweepDirection: if sweep {
                            D2D1_SWEEP_DIRECTION_CLOCKWISE
                        } else {
                            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                        },
                        arcSize: if large {
                            D2D1_ARC_SIZE_LARGE
                        } else {
                            D2D1_ARC_SIZE_SMALL
                        },
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
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// 解码好的图片（头像），straight alpha 的 RGBA。
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// (图片指针, 目标设备像素宽, 高) → 预缩放好的位图；Weak 判断图片还活着、指针没被复用。
type BitmapCache = HashMap<(usize, u32, u32), (std::rc::Weak<Image>, ID2D1Bitmap)>;

/// 绑定在某个渲染目标上的画布。图片位图跟着渲染目标走，换目标时整个丢掉重建。
pub struct Canvas {
    rt: ID2D1RenderTarget,
    shared: std::rc::Rc<Shared>,
    brush: ID2D1SolidColorBrush,
    bitmaps: RefCell<BitmapCache>,
    pub scale: f32,
}

impl Canvas {
    pub fn new(rt: ID2D1RenderTarget, shared: std::rc::Rc<Shared>, scale: f32) -> Result<Self> {
        // SAFETY: rt 是本函数拥有的活 COM 引用；SetDpi / SetAntialiasMode / CreateSolidColorBrush 只读写它自己的状态。
        let brush = unsafe {
            rt.SetDpi(96.0 * scale, 96.0 * scale);
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.CreateSolidColorBrush(&color(Rgba::TRANSPARENT), None)?
        };
        Ok(Self {
            rt,
            shared,
            brush,
            bitmaps: RefCell::new(HashMap::new()),
            scale,
        })
    }

    /// 开始一帧；守卫的 finish 拿 EndDraw 结果，提前返回时 Drop 补 EndDraw。
    pub fn begin_draw(&self) -> DrawGuard<'_> {
        DrawGuard::begin(&self.rt)
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
        let rr = D2D1_ROUNDED_RECT {
            rect: r.d2d(),
            radiusX: radius,
            radiusY: radius,
        };
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
        let rr = D2D1_ROUNDED_RECT {
            rect: outer.d2d(),
            radiusX: radius,
            radiusY: radius,
        };
        // SAFETY: D2D 绘制调用，参数是栈上的值。
        unsafe { self.rt.DrawRoundedRectangle(&rr, self.paint(c), px, None) };
    }

    /// 1px 横线（border-top），按设备像素对齐。1px 的线落在设备像素上才不发虚。
    pub fn hline(&self, x: f32, right: f32, y: f32, c: Rgba) {
        let px = (self.scale.floor().max(1.0)) / self.scale;
        let y = self.snap(y);
        self.fill(Rect::new(x, y, right - x, px), c);
    }

    pub fn circle(&self, cx: f32, cy: f32, radius: f32, c: Rgba) {
        if c.a <= 0.0 {
            return;
        }
        let e = D2D1_ELLIPSE {
            point: v((cx, cy)),
            radiusX: radius,
            radiusY: radius,
        };
        // SAFETY: D2D 绘制调用，参数是栈上的值。
        unsafe { self.rt.FillEllipse(&e, self.paint(c)) };
    }

    pub fn text(&self, text: &TextBox, x: f32, y: f32, c: Rgba) {
        // SAFETY: D2D 绘制调用，排版对象活到调用结束。
        unsafe {
            self.rt.DrawTextLayout(
                v((x, y)),
                &text.layout,
                self.paint(c),
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );
        }
    }

    /// lucide 图标：24 单位的画板缩放到 `size`，描边宽度按画板单位给（和 lucide 的 strokeWidth 一致）。
    #[expect(clippy::too_many_arguments, reason = "绘制原语按坐标、样式平铺传参")]
    pub fn icon(&self, icon: Icon, x: f32, y: f32, size: f32, stroke: f32, c: Rgba, filled: bool) {
        let Ok(geometry) = self.shared.icon(icon) else {
            return;
        };
        let k = size / 24.0;
        let transform = Matrix3x2 {
            M11: k,
            M12: 0.0,
            M21: 0.0,
            M22: k,
            M31: x,
            M32: y,
        };
        let _transform = TransformGuard::set(&self.rt, &transform);
        let brush = self.paint(c);
        // SAFETY: D2D 绘制调用，几何、画刷、描边样式都是活着的 COM 引用。
        unsafe {
            if filled {
                self.rt.FillGeometry(&geometry, brush, None);
            }
            self.rt
                .DrawGeometry(&geometry, brush, stroke, &self.shared.round_stroke);
        }
    }

    /// 旋转着画（加载圈）：绕图标中心转 `degrees`。
    #[expect(clippy::too_many_arguments, reason = "绘制原语按坐标、样式平铺传参")]
    pub fn icon_rotated(
        &self,
        icon: Icon,
        x: f32,
        y: f32,
        size: f32,
        stroke: f32,
        c: Rgba,
        degrees: f32,
    ) {
        let Ok(geometry) = self.shared.icon(icon) else {
            return;
        };
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
        let _transform = TransformGuard::set(&self.rt, &m);
        // SAFETY: D2D 绘制调用，几何、画刷、描边样式都是活着的 COM 引用。
        unsafe {
            self.rt
                .DrawGeometry(&geometry, self.paint(c), stroke, &self.shared.round_stroke);
        }
    }

    /// 左上到右下的两色渐变圆角块（头像加载不到时的底色）。
    pub fn gradient_round(&self, r: Rect, radius: f32, from: Rgba, to: Rgba) {
        let r = self.snap_rect(r);
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: color(from),
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: color(to),
            },
        ];
        // SAFETY: D2D 资源创建与绘制，参数是栈上的值。
        unsafe {
            let Ok(collection) = self.rt.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) else {
                return;
            };
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: v((r.x, r.y)),
                endPoint: v((r.right(), r.bottom())),
            };
            let Ok(brush) = self.rt.CreateLinearGradientBrush(&props, None, &collection) else {
                return;
            };
            let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
            self.rt.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: r.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &brush,
            );
        }
    }

    /// 图片铺满 `r` 并裁成圆角（object-fit: cover）。位图预先在 CPU 上缩到目标设备像素，
    /// 大图直接让 D2D 线性缩小会有明显混叠（256px 的 logo 画到 36px）。
    pub fn image_round(&self, image: &std::rc::Rc<Image>, r: Rect, radius: f32) {
        let r = self.snap_rect(r);
        let w_px = (r.w * self.scale).round().max(1.0) as u32;
        let h_px = (r.h * self.scale).round().max(1.0) as u32;
        let Some(bitmap) = self.bitmap(image, w_px, h_px) else {
            return;
        };
        // SAFETY: D2D 资源创建与绘制；位图 DPI = 96 × scale，1 位图像素正好是 1 设备像素，画刷只需平移。
        unsafe {
            use windows::Win32::Graphics::Direct2D::{
                D2D1_BITMAP_BRUSH_PROPERTIES, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
            };
            let props = D2D1_BITMAP_BRUSH_PROPERTIES {
                extendModeX: D2D1_EXTEND_MODE_CLAMP,
                extendModeY: D2D1_EXTEND_MODE_CLAMP,
                interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
            };
            let Ok(brush) = self.rt.CreateBitmapBrush(&bitmap, Some(&props), None) else {
                return;
            };
            brush.SetTransform(&Matrix3x2 {
                M11: 1.0,
                M12: 0.0,
                M21: 0.0,
                M22: 1.0,
                M31: r.x,
                M32: r.y,
            });
            let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
            self.rt.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: r.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &brush,
            );
        }
    }

    fn bitmap(&self, image: &std::rc::Rc<Image>, w_px: u32, h_px: u32) -> Option<ID2D1Bitmap> {
        let key = (std::rc::Rc::as_ptr(image) as usize, w_px, h_px);
        if let Some((weak, bitmap)) = self.bitmaps.borrow().get(&key)
            && weak
                .upgrade()
                .is_some_and(|alive| std::rc::Rc::ptr_eq(&alive, image))
        {
            return Some(bitmap.clone());
        }
        let scaled = cover_resize(image, w_px, h_px)?;
        // D2D 要预乘的 BGRA
        let mut bgra = Vec::with_capacity(scaled.len());
        for px in scaled.chunks_exact(4) {
            let &[r, g, b, a] = px else { continue };
            let mul = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
            bgra.extend_from_slice(&[mul(b), mul(g), mul(r), a]);
        }
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0 * self.scale,
            dpiY: 96.0 * self.scale,
        };
        // D2D 按 pitch × height 从指针读，像素数对不上时宁可不画，也不能越界读
        let pitch = w_px.checked_mul(4)?;
        let expected = (pitch as usize).checked_mul(h_px as usize)?;
        if expected == 0 || bgra.len() != expected {
            return None;
        }
        // SAFETY: bgra 活到调用结束，上面已校验长度正好是 pitch × height 字节，紧密排列无行间填充。
        let bitmap = unsafe {
            self.rt
                .CreateBitmap(
                    D2D_SIZE_U {
                        width: w_px,
                        height: h_px,
                    },
                    Some(bgra.as_ptr().cast()),
                    pitch,
                    &props,
                )
                .ok()?
        };
        let mut cache = self.bitmaps.borrow_mut();
        cache.retain(|_, (weak, _)| weak.strong_count() > 0);
        cache.insert(key, (std::rc::Rc::downgrade(image), bitmap.clone()));
        Some(bitmap)
    }

    /// 裁到 `r`，返回的守卫离开作用域时弹出。
    pub fn clip(&self, r: Rect) -> ClipGuard<'_> {
        ClipGuard::push(&self.rt, self.snap_rect(r).d2d())
    }
}

/// object-fit: cover：按目标宽高比居中裁，再 Lanczos3 缩到目标尺寸。返回非预乘 RGBA。
fn cover_resize(image: &Image, w: u32, h: u32) -> Option<Vec<u8>> {
    let src = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())?;
    let (iw, ih) = (u64::from(image.width), u64::from(image.height));
    if iw == 0 || ih == 0 || w == 0 || h == 0 {
        return None;
    }
    // 交叉相乘比较宽高比，避开浮点：源比目标宽就裁左右，否则裁上下
    let (cw, ch) = if iw * u64::from(h) > ih * u64::from(w) {
        ((ih * u64::from(w) / u64::from(h)).max(1), ih)
    } else {
        (iw, (iw * u64::from(h) / u64::from(w)).max(1))
    };
    let (cx, cy) = ((iw - cw) / 2, (ih - ch) / 2);
    let cropped =
        image::imageops::crop_imm(&src, cx as u32, cy as u32, cw as u32, ch as u32).to_image();
    if cropped.width() == w && cropped.height() == h {
        return Some(cropped.into_raw());
    }
    Some(image::imageops::resize(&cropped, w, h, image::imageops::FilterType::Lanczos3).into_raw())
}

/// 离屏画一帧，返回 RGBA（不透明）。测试截图用。
#[cfg(test)]
pub(crate) fn render_offscreen(
    w_px: i32,
    h_px: i32,
    scale: f32,
    draw: impl FnOnce(&Canvas),
) -> Option<Vec<u8>> {
    let shared = shared().ok()?;
    let factory = shared.d2d.clone();
    let mut drawn = false;
    let pixels = super::sys::testing::render_offscreen(&factory, w_px, h_px, scale, |rt| {
        let Ok(canvas) = Canvas::new(rt, shared, scale) else {
            return;
        };
        let frame = canvas.begin_draw();
        draw(&canvas);
        drawn = frame.finish().is_ok();
    })?;
    drawn.then_some(pixels)
}

#[cfg(test)]
mod font_lifetime_tests {
    use super::*;

    #[test]
    fn layout_keeps_fonts_registered_after_text_system_is_released() {
        let text = TextSystem::new().unwrap();
        let fonts = Rc::downgrade(&text.fonts);
        let factory = text.fonts.factory.clone();
        let loader = text.fonts.loader.clone();
        let layout = text
            .layout("字体生命周期", TextStyle::sans(14.0, 400, 20.0), 200.0)
            .unwrap();
        let copy = layout.clone();
        drop(text);
        drop(layout);
        assert!(fonts.upgrade().is_some());
        // SAFETY: copy 仍持有 layout 和字体注册所有者。
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { copy.layout.GetMetrics(&mut metrics) }.unwrap();
        assert!(metrics.width > 0.0);
        drop(copy);
        assert!(fonts.upgrade().is_none());
        // SAFETY: 仅检查注销已完成；factory 和 loader 的 COM 引用仍在本测试内存活。
        assert!(unsafe { factory.UnregisterFontFileLoader(&loader) }.is_err());
    }
}
