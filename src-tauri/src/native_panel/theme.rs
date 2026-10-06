// 面板配色和圆角：按 app-settings 的 theme / radiusStyle 选 tokens.css 里对应的一套，WebView 版也是这么取的。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }

    pub const fn alpha(self, a: f32) -> Self {
        Self {
            a: self.a * a,
            ..self
        }
    }

    /// srgb 里按比例混色（绘制浅色淡淡的状态背景用）
    pub fn mix(self, other: Rgba, amount: f32) -> Rgba {
        let t = amount.clamp(0.0, 1.0);
        Rgba {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

/// 主窗口解析完样式后推过来的面板配色；全 Plain struct，ts-rs 直出。
/// 传 None 的字段回落到当前主题默认值。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct TrayPanelPalette {
    pub elevated: Option<String>,
    pub muted: Option<String>,
    pub text: Option<String>,
    pub text_secondary: Option<String>,
    pub text_tertiary: Option<String>,
    pub text_disabled: Option<String>,
    pub brand: Option<String>,
    pub brand_soft: Option<String>,
    pub success: Option<String>,
    pub danger: Option<String>,
    pub danger_bg: Option<String>,
    pub border_subtle: Option<String>,
    pub radius_sm: Option<f32>,
    pub radius_md: Option<f32>,
}

/// 面板用的全套颜色，都来自 token；新主题切换时整组换掉。
/// radius_sm / radius_md 已经乘过 radiusStyle 的系数（tailwind 的 rounded-sm / rounded-md）。
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub elevated: Rgba,
    pub muted: Rgba,
    pub text: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub text_disabled: Rgba,
    pub brand: Rgba,
    pub brand_soft: Rgba,
    pub success: Rgba,
    pub danger: Rgba,
    pub danger_bg: Rgba,
    pub border_subtle: Rgba,
    pub radius_sm: f32,
    pub radius_md: f32,
}

// tokens.css 浅色 / 深色（受 OS 设置的 data-theme=auto 分支）
const LIGHT: Theme = Theme {
    elevated: Rgba::rgb(0xff, 0xf8, 0xf0),
    muted: Rgba::rgb(0xf4, 0xef, 0xe7),
    text: Rgba::rgb(0x2c, 0x1f, 0x18),
    text_secondary: Rgba::rgb(0x5c, 0x53, 0x4b),
    text_tertiary: Rgba::rgb(0x9a, 0x8e, 0x84),
    text_disabled: Rgba::rgb(0xb8, 0xad, 0x9b),
    brand: Rgba::rgb(0xff, 0x6b, 0x3d),
    brand_soft: Rgba::rgb(0xff, 0xe9, 0xcf),
    success: Rgba::rgb(0x4f, 0xb4, 0x77),
    danger: Rgba::rgb(0xe8, 0x5b, 0x57),
    danger_bg: Rgba::rgb(0xe8, 0x5b, 0x57).alpha(0.12),
    border_subtle: Rgba::rgb(0x2c, 0x1f, 0x18).alpha(0.06),
    radius_sm: 8.0,
    radius_md: 12.0,
};

const DARK: Theme = Theme {
    elevated: Rgba::rgb(0x3a, 0x37, 0x34),
    muted: Rgba::rgb(0x33, 0x30, 0x2d),
    text: Rgba::rgb(0xf5, 0xf1, 0xed),
    text_secondary: Rgba::rgb(0xc4, 0xbd, 0xb5),
    text_tertiary: Rgba::rgb(0x96, 0x8e, 0x86),
    text_disabled: Rgba::rgb(0x5e, 0x58, 0x52),
    brand: Rgba::rgb(0xff, 0x8a, 0x57),
    // 深色下 brand-soft 是 color-mix(brand 16%, #2e2b28)
    brand_soft: Rgba::rgb(0x48, 0x40, 0x3a),
    success: Rgba::rgb(0x4f, 0xb4, 0x77),
    danger: Rgba::rgb(0xe8, 0x5b, 0x57),
    danger_bg: Rgba::rgb(0xe8, 0x5b, 0x57).alpha(0.12),
    border_subtle: Rgba::rgb(0xff, 0xf5, 0xeb).alpha(0.08),
    radius_sm: 8.0,
    radius_md: 12.0,
};

// Catppuccin 四套，值逐个抄 tokens.css 对应的 :root[data-theme=...] 块；
// 深色三套的 brand-soft 是 color-mix(brand 16%, surface-card)，这里存算好的结果。
const LATTE: Theme = Theme {
    elevated: Rgba::rgb(0xff, 0xff, 0xff),
    muted: Rgba::rgb(0xcc, 0xd0, 0xda),
    text: Rgba::rgb(0x4c, 0x4f, 0x69),
    text_secondary: Rgba::rgb(0x5c, 0x5f, 0x77),
    text_tertiary: Rgba::rgb(0x6c, 0x6f, 0x85),
    text_disabled: Rgba::rgb(0x9c, 0xa0, 0xb0),
    brand: Rgba::rgb(0x88, 0x39, 0xef),
    brand_soft: Rgba::rgb(0xed, 0xe4, 0xff),
    success: Rgba::rgb(0x40, 0xa0, 0x2b),
    danger: Rgba::rgb(0xd2, 0x0f, 0x39),
    danger_bg: Rgba::rgb(0xd2, 0x0f, 0x39).alpha(0.12),
    border_subtle: Rgba::rgb(0x4c, 0x4f, 0x69).alpha(0.06),
    radius_sm: 8.0,
    radius_md: 12.0,
};

const FRAPPE: Theme = Theme {
    elevated: Rgba::rgb(0x41, 0x45, 0x59),
    muted: Rgba::rgb(0x41, 0x45, 0x59),
    text: Rgba::rgb(0xc6, 0xd0, 0xf5),
    text_secondary: Rgba::rgb(0xb5, 0xbf, 0xe2),
    text_tertiary: Rgba::rgb(0xa5, 0xad, 0xce),
    text_disabled: Rgba::rgb(0x73, 0x79, 0x94),
    brand: Rgba::rgb(0xca, 0x9e, 0xe6),
    brand_soft: Rgba::rgb(0x43, 0x3e, 0x57),
    success: Rgba::rgb(0xa6, 0xd1, 0x89),
    danger: Rgba::rgb(0xe7, 0x82, 0x84),
    danger_bg: Rgba::rgb(0xe7, 0x82, 0x84).alpha(0.14),
    border_subtle: Rgba::rgb(0xc6, 0xd0, 0xf5).alpha(0.08),
    radius_sm: 8.0,
    radius_md: 12.0,
};

const MACCHIATO: Theme = Theme {
    elevated: Rgba::rgb(0x36, 0x3a, 0x4f),
    muted: Rgba::rgb(0x36, 0x3a, 0x4f),
    text: Rgba::rgb(0xca, 0xd3, 0xf5),
    text_secondary: Rgba::rgb(0xb8, 0xc0, 0xe0),
    text_tertiary: Rgba::rgb(0xa5, 0xad, 0xcb),
    text_disabled: Rgba::rgb(0x6e, 0x73, 0x8d),
    brand: Rgba::rgb(0xc6, 0xa0, 0xf6),
    brand_soft: Rgba::rgb(0x39, 0x34, 0x50),
    success: Rgba::rgb(0xa6, 0xda, 0x95),
    danger: Rgba::rgb(0xed, 0x87, 0x96),
    danger_bg: Rgba::rgb(0xed, 0x87, 0x96).alpha(0.14),
    border_subtle: Rgba::rgb(0xca, 0xd3, 0xf5).alpha(0.08),
    radius_sm: 8.0,
    radius_md: 12.0,
};

const MOCHA: Theme = Theme {
    elevated: Rgba::rgb(0x31, 0x32, 0x44),
    muted: Rgba::rgb(0x31, 0x32, 0x44),
    text: Rgba::rgb(0xcd, 0xd6, 0xf4),
    text_secondary: Rgba::rgb(0xba, 0xc2, 0xde),
    text_tertiary: Rgba::rgb(0xa6, 0xad, 0xc8),
    text_disabled: Rgba::rgb(0x6c, 0x70, 0x86),
    brand: Rgba::rgb(0xcb, 0xa6, 0xf7),
    brand_soft: Rgba::rgb(0x35, 0x2f, 0x47),
    success: Rgba::rgb(0xa6, 0xe3, 0xa1),
    danger: Rgba::rgb(0xf3, 0x8b, 0xa8),
    danger_bg: Rgba::rgb(0xf3, 0x8b, 0xa8).alpha(0.14),
    border_subtle: Rgba::rgb(0xcd, 0xd6, 0xf4).alpha(0.08),
    radius_sm: 8.0,
    radius_md: 12.0,
};

static PALETTE: Mutex<Option<Theme>> = Mutex::new(None);

/// app-settings.json 里的 uiPreferences.theme / radiusStyle，打开面板时由业务层读一次塞进来。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiPreferences {
    pub theme: String,
    pub radius_style: String,
}

static PREFERENCES: Mutex<Option<UiPreferences>> = Mutex::new(None);

pub fn set_ui_preferences(prefs: UiPreferences) {
    *PREFERENCES.lock().unwrap_or_else(|p| p.into_inner()) = Some(prefs);
}

/// 对齐 src-ui/core/design/radius.ts 的 RADIUS_SCALE。
fn radius_scale(style: &str) -> f32 {
    match style {
        "square" => 0.5,
        "round" => 1.5,
        _ => 1.0,
    }
}

/// 主题 id → 内置色板。社区主题、auto 和没读到配置时跟系统明暗，和 tokens.css 的默认分支一致。
fn builtin(theme_id: &str) -> Theme {
    match theme_id {
        "light" => LIGHT,
        "dark" => DARK,
        "latte" => LATTE,
        "frappe" => FRAPPE,
        "macchiato" => MACCHIATO,
        "mocha" => MOCHA,
        _ if system_dark() => DARK,
        _ => LIGHT,
    }
}

/// 解析 CSS 颜色：#rgb / #rrggbb / rgb / rgba / 以及 color-mix(in srgb, X n%, X) 这类两色混合。
/// 数量对不上就返回 None，写错色值时直接落兜底色。
pub fn parse_color(value: &str) -> Option<Rgba> {
    let v = value.trim();
    fn digit(s: u8) -> Option<f32> {
        (s as char).to_digit(16).map(|v| v as f32 / 15.0)
    }
    fn pair(hex: &str, at: usize) -> Option<f32> {
        // get 而不是切片：非 ASCII 输入切在字符中间时返回 None，不 panic
        let pair = hex.get(at..at + 2)?;
        u8::from_str_radix(pair, 16).ok().map(|v| v as f32 / 255.0)
    }
    if let Some(hex) = v.strip_prefix('#') {
        return match *hex.as_bytes() {
            [r, g, b] => Some(Rgba {
                r: digit(r)?,
                g: digit(g)?,
                b: digit(b)?,
                a: 1.0,
            }),
            [_, _, _, _, _, _] => Some(Rgba {
                r: pair(hex, 0)?,
                g: pair(hex, 2)?,
                b: pair(hex, 4)?,
                a: 1.0,
            }),
            _ => None,
        };
    }
    if let Some(inner) = v
        .strip_prefix("color-mix(in srgb,")
        .and_then(|s| s.strip_suffix(')'))
    {
        // getComputedStyle 里 dark 的 color-mix 已经算成 rgba()，但 color-mix 也会原样带出来
        let mut parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        let second = parts.pop()?;
        let first = parts.pop()?;
        // 「X n%」和「X」都算；X 本身可以是 rgb(...)
        let (left, amount) = match first.rsplit_once(' ') {
            Some((color_part, percent_part)) => (
                color_part.trim(),
                percent_part.trim_end_matches('%').parse::<f32>().ok()? / 100.0,
            ),
            None => (first.trim(), 0.5),
        };
        let base = if second.trim().eq_ignore_ascii_case("transparent") {
            Rgba::TRANSPARENT
        } else {
            parse_color(second.trim())?
        };
        let top = if left.is_empty() {
            Rgba::TRANSPARENT
        } else {
            parse_color(left)?
        };
        // 和 CSS 一致：两侧 alpha 都作用在各自的贡献里
        return Some(Rgba {
            r: top.r * (amount * top.a) + base.r * ((1.0 - amount) * base.a),
            g: top.g * (amount * top.a) + base.g * ((1.0 - amount) * base.a),
            b: top.b * (amount * top.a) + base.b * ((1.0 - amount) * base.a),
            a: top.a * amount + base.a * (1.0 - amount),
        });
    }
    if v.starts_with("rgb") {
        let inner = v.get(v.find('(')? + 1..v.rfind(')')?)?.replace('/', " ");
        let mut iter = inner.split([' ', ',']).filter(|p| !p.is_empty());
        let ch = |s: &str| {
            s.trim_end_matches('%')
                .parse::<f32>()
                .ok()
                .map(|v| if s.ends_with('%') { v * 2.55 } else { v } / 255.0)
        };
        let r = ch(iter.next()?)?;
        let g = ch(iter.next()?)?;
        let b = ch(iter.next()?)?;
        // alpha 是 0..1 的小数或百分比，不按 0..255 折算
        let a = iter
            .next()
            .and_then(|s| match s.strip_suffix('%') {
                Some(p) => p.parse::<f32>().ok().map(|v| v / 100.0),
                None => s.parse::<f32>().ok(),
            })
            .unwrap_or(1.0);
        return Some(Rgba { r, g, b, a });
    }
    None
}

fn system_dark() -> bool {
    crate::windows_ui::system_prefers_dark()
}

/// 当前面板主题：主窗口推过配色就用推的，否则按 app-settings 的主题选内置色板；圆角按 radiusStyle 缩放。
pub fn theme() -> Theme {
    let prefs = PREFERENCES
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .unwrap_or_default();
    let pushed = *PALETTE.lock().unwrap_or_else(|p| p.into_inner());
    let mut theme = pushed.unwrap_or_else(|| builtin(&prefs.theme));
    if pushed.is_none() {
        let k = radius_scale(&prefs.radius_style);
        theme.radius_sm *= k;
        theme.radius_md *= k;
    }
    theme
}

impl Theme {
    /// 底色亮度低于一半算深色，DWM 边框跟着切。
    pub fn is_dark(&self) -> bool {
        let c = self.elevated;
        0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b < 0.5
    }
}

/// 主窗口每帧不需要；主题变化或主窗就绪时推一次。
pub fn sync_palette(palette: &TrayPanelPalette) {
    let base = theme();
    let pick = |value: &Option<String>, fallback: Rgba| {
        value.as_deref().and_then(parse_color).unwrap_or(fallback)
    };
    let theme = Theme {
        elevated: pick(&palette.elevated, base.elevated),
        muted: pick(&palette.muted, base.muted),
        text: pick(&palette.text, base.text),
        text_secondary: pick(&palette.text_secondary, base.text_secondary),
        text_tertiary: pick(&palette.text_tertiary, base.text_tertiary),
        text_disabled: pick(&palette.text_disabled, base.text_disabled),
        brand: pick(&palette.brand, base.brand),
        brand_soft: pick(&palette.brand_soft, base.brand_soft),
        success: pick(&palette.success, base.success),
        danger: pick(&palette.danger, base.danger),
        danger_bg: pick(&palette.danger_bg, base.danger_bg),
        border_subtle: pick(&palette.border_subtle, base.border_subtle),
        radius_sm: palette.radius_sm.unwrap_or(base.radius_sm),
        radius_md: palette.radius_md.unwrap_or(base.radius_md),
    };
    *PALETTE.lock().unwrap_or_else(|p| p.into_inner()) = Some(theme);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_rgb() {
        assert_eq!(parse_color("#fff8f0"), Some(Rgba::rgb(0xff, 0xf8, 0xf0)));
        assert_eq!(parse_color("rgb(44, 31, 24)").unwrap().a, 1.0);
        let c = parse_color("rgba(255, 245, 235, 0.08)").unwrap();
        assert!((c.a - 0.08).abs() < 0.01);
    }

    #[test]
    fn parses_color_mix_against_transparent_and_hex() {
        let c = parse_color("color-mix(in srgb, #e85b57 12%, transparent)").unwrap();
        assert!((c.a - 0.12).abs() < 0.01);
        // dark 的 brand-soft：mix(brand 16%, #2e2b28)
        let c = parse_color("color-mix(in srgb, #ff8a57 16%, #2e2b28)").unwrap();
        assert!(c.r > 0.28 && c.r < 0.34 && (c.a - 1.0).abs() < 0.01);
    }

    #[test]
    fn malformed_colors_do_not_panic() {
        assert_eq!(parse_color("#é1"), None);
        assert_eq!(parse_color("#a\u{e9}123"), None);
        assert_eq!(parse_color("rgb)1,2,3("), None);
        assert_eq!(parse_color("rgba(1, 2, 3, 50%)").map(|c| c.a), Some(0.5));
    }

    #[test]
    fn builtin_themes_follow_preferences() {
        assert_eq!(builtin("latte").elevated, Rgba::rgb(0xff, 0xff, 0xff));
        assert!(!builtin("latte").is_dark());
        assert!(builtin("mocha").is_dark());
        assert_eq!(radius_scale("square"), 0.5);
        assert_eq!(radius_scale("whatever"), 1.0);
    }

    #[test]
    fn palette_fill_from_push() {
        sync_palette(&TrayPanelPalette {
            brand: Some("#123456".into()),
            ..Default::default()
        });
        let t = theme();
        assert_eq!(t.brand, Rgba::rgb(0x12, 0x34, 0x56));
    }
}
