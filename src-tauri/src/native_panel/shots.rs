// 测试截图：设了 NCD_PANEL_SHOTS_DIR 才落盘，离屏渲染看布局，真窗口截屏看圆角和阴影。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::gfx::{Canvas, render_offscreen};
use super::sys::Hwnd;
use super::sys::testing;
use super::theme::theme;
use super::window::{NativePanel, PanelContent, PositionFn};

pub fn dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("NCD_PANEL_SHOTS_DIR")?);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn save(name: &str, w: u32, h: u32, rgba: Vec<u8>) {
    let Some(dir) = dir() else { return };
    let Some(img) = image::RgbaImage::from_raw(w, h, rgba) else {
        return;
    };
    img.save(dir.join(format!("{name}.png")))
        .expect("write png");
}

/// 按 260 × h DIP 离屏画一帧。`paint` 前已经铺好面板底色。
pub fn offscreen(name: &str, scale: f32, height_dip: f32, paint: impl FnOnce(&Canvas)) {
    let w = (260.0 * scale).round() as i32;
    let h = (height_dip * scale).ceil() as i32;
    let pixels = render_offscreen(w, h, scale, |canvas| {
        canvas.clear(theme().elevated);
        paint(canvas);
    })
    .expect("offscreen render");
    save(name, w as u32, h as u32, pixels);
}

/// 建真面板显示在 anchor，等 DWM 合成完截屏（四周多截 24px 看阴影）。
pub fn window(
    name: &str,
    content: Box<dyn PanelContent>,
    anchor: (i32, i32),
    position: PositionFn,
) {
    testing::per_monitor_dpi_thread();
    let panel = NativePanel::create(anchor, 260.0, content).expect("create panel");
    panel.show_with(anchor, false, position);
    let deadline = Instant::now() + Duration::from_millis(700);
    while Instant::now() < deadline {
        testing::pump_messages();
        std::thread::sleep(Duration::from_millis(16));
    }
    testing::dwm_flush();
    let rect = testing::window_rect(Hwnd::from_raw(panel.hwnd())).expect("window rect");
    let m = 24;
    let (x, y) = (rect.left - m, rect.top - m);
    let (w, h) = (
        rect.right - rect.left + 2 * m,
        rect.bottom - rect.top + 2 * m,
    );
    if let Some(pixels) = testing::capture_screen(x, y, w, h) {
        save(name, w as u32, h as u32, pixels);
    }
    panel.destroy();
}
