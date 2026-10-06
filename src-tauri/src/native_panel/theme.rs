// 面板配色和圆角：对齐 src-ui 的 design token。
//
// 默认是主窗口打开前（托盘是唯一入口时）的兜底：灯 / 暗两套内置色随系统明暗走。
// 主窗口起来后把解析完的颜色推过来（`native_panel::sync_palette`），社区主题也生效。

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
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }

    pub const fn alpha(self, a: f32) -> Self {
        Self { a: self.a * a, ..self }
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

static PALETTE: Mutex<Option<Theme>> = Mutex::new(None);

/// 解析 CSS 颜色：#rgb / #rrggbb / rgb / rgba / 以及 color-mix(in srgb, X n%, X) 这类两色混合。
/// 数量对不上就返回 None，写错色值时直接落兜底色。
pub fn parse_color(value: &str) -> Option<Rgba> {
    let v = value.trim();
    fn digit(s: u8) -> Option<f32> {
        (s as char).to_digit(16).map(|v| v as f32 / 15.0)
    }
    fn pair(pair: &str) -> Option<f32> {
        u8::from_str_radix(pair, 16).ok().map(|v| v as f32 / 255.0)
    }
    if let Some(hex) = v.strip_prefix('#') {
        return match hex.len() {
            3 => Some(Rgba {
                r: digit(hex.as_bytes()[0])?,
                g: digit(hex.as_bytes()[1])?,
                b: digit(hex.as_bytes()[2])?,
                a: 1.0,
            }),
            6 => Some(Rgba { r: pair(&hex[0..2])?, g: pair(&hex[2..4])?, b: pair(&hex[4..6])?, a: 1.0 }),
            _ => None,
        };
    }
    if let Some(inner) = v.strip_prefix("color-mix(in srgb,").and_then(|s| s.strip_suffix(')')) {
        // getComputedStyle 里 dark 的 color-mix 已经算成 rgba()，但 color-mix 也会原样带出来
        let mut parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        let second = parts.pop()?;
        let first = parts.pop()?;
        // 「X n%」和「X」都算；X 本身可以是 rgb(...)
        let (left, amount) = match first.rsplit_once(' ') {
            Some((color_part, percent_part)) => (color_part.trim(), percent_part.trim_end_matches('%').parse::<f32>().ok()? / 100.0),
            None => (first.trim(), 0.5),
        };
        let base = if second.trim().eq_ignore_ascii_case("transparent") { Rgba::TRANSPARENT } else { parse_color(second.trim())? };
        let top = if left.is_empty() { Rgba::TRANSPARENT } else { parse_color(left)? };
        // 和 CSS 一致：两侧 alpha 都作用在各自的贡献里
        return Some(Rgba {
            r: top.r * (amount * top.a) + base.r * ((1.0 - amount) * base.a),
            g: top.g * (amount * top.a) + base.g * ((1.0 - amount) * base.a),
            b: top.b * (amount * top.a) + base.b * ((1.0 - amount) * base.a),
            a: top.a * amount + base.a * (1.0 - amount),
        });
    }
    if v.starts_with("rgb") {
        let inner = v[v.find('(')? + 1..v.rfind(')')?].replace('/', " ");
        let mut iter = inner.split([' ', ',']).filter(|p| !p.is_empty());
        let ch = |s: &str| s.trim_end_matches('%').parse::<f32>().ok().map(|v| if s.ends_with('%') { v * 2.55 } else { v } / 255.0);
        let r = ch(iter.next()?)?;
        let g = ch(iter.next()?)?;
        let b = ch(iter.next()?)?;
        let a = iter.next().and_then(|s| ch(s)).unwrap_or(1.0);
        return Some(Rgba { r, g, b, a });
    }
    None
}

fn system_dark() -> bool {
    crate::windows_ui::system_prefers_dark()
}

/// 当前面板主题：主窗口推过用推的，否则按系统明暗。
pub fn theme() -> Theme {
    PALETTE.lock().unwrap_or_else(|p| p.into_inner()).unwrap_or(if system_dark() { DARK } else { LIGHT })
}

/// 主窗口每帧不需要；主题变化或主窗就绪时推一次。
pub fn sync_palette(palette: &TrayPanelPalette) {
    let base = theme();
    let pick = |value: &Option<String>, fallback: Rgba| value.as_deref().and_then(parse_color).unwrap_or(fallback);
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
    fn palette_fill_from_push() {
        sync_palette(&TrayPanelPalette { brand: Some("#123456".into()), ..Default::default() });
        let t = theme();
        assert_eq!(t.brand, Rgba::rgb(0x12, 0x34, 0x56));
    }
}
