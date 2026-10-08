// 用 DirectWrite 渲染的真实中文和英文像素验证滚动拼接。
use super::{
    capture::render_offscreen,
    stitch::{Append, Frame, Stitcher},
};
use crate::native_panel::{
    gfx::{Canvas, TextStyle, shared},
    theme::Rgba,
};

fn rendered_page(width: u32, height: u32, paint: impl FnOnce(&Canvas)) -> Frame {
    let mut bgra = render_offscreen(width, height, |target| {
        let canvas = Canvas::new(target, shared().map_err(|e| e.to_string())?, 1.0)
            .map_err(|e| e.to_string())?;
        let draw = canvas.begin_draw();
        canvas.clear(Rgba::rgb(250, 250, 252));
        paint(&canvas);
        draw.finish().map_err(|e| e.to_string())
    })
    .unwrap();
    // 离屏导出使用 RGBA，拼接和真实屏幕采集统一使用 BGRA。
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Frame {
        width,
        height,
        bgra,
    }
}

fn text(canvas: &Canvas, value: &str, x: f32, y: f32, width: f32, size: f32) {
    let layout = canvas
        .text_system()
        .layout(value, TextStyle::sans(size, 400, size + 6.0), width)
        .unwrap();
    canvas.text(&layout, x, y, Rgba::rgb(55, 59, 73));
}

pub(super) fn document(width: u32, height: u32) -> Frame {
    rendered_page(width, height, |canvas| {
        for line in 0..height / 38 {
            text(
                canvas,
                &format!(
                    "第 {:03} 条消息：明天 {} 点讨论截图体验",
                    line + 1,
                    line % 12 + 1
                ),
                18.0,
                9.0 + line as f32 * 38.0,
                width as f32 - 36.0,
                14.0,
            );
        }
    })
}

fn crop(page: &Frame, offset: u32, height: u32) -> Frame {
    let stride = page.width as usize * 4;
    Frame {
        width: page.width,
        height,
        bgra: page.bgra[offset as usize * stride..(offset + height) as usize * stride].to_vec(),
    }
}

fn assert_prefix(result: &Frame, page: &Frame, height: u32) {
    assert_eq!(result.width, page.width);
    assert_eq!(result.height, height, "累计高度必须等于视口加真实滚动距离");
    let expected = &page.bgra[..height as usize * page.width as usize * 4];
    assert_eq!(result.bgra.len(), expected.len());
    if let Some(at) = result.bgra.iter().zip(expected).position(|(a, b)| a != b) {
        panic!(
            "拼接改变了原文像素：x={} y={} channel={}",
            at / 4 % page.width as usize,
            at / 4 / page.width as usize,
            at % 4
        );
    }
}

fn append_exact(stitcher: &mut Stitcher, frame: Frame, added: u32) {
    assert_eq!(stitcher.append(frame).unwrap(), Append::Added(added));
}

#[test]
fn rendered_text_page_preserves_pixels_across_scroll_and_return() {
    let _resources = super::render::SharedScope;
    let width = 1024;
    let page_height = 2400;
    let viewport = 720;
    let page = render_offscreen(width, page_height, |target| {
        let canvas = Canvas::new(target, shared().map_err(|e| e.to_string())?, 1.0)
            .map_err(|e| e.to_string())?;
        let draw = canvas.begin_draw();
        canvas.clear(Rgba::rgb(250, 250, 252));
        for line in 0..64 {
            let title = format!(
                "{:03}  聊天记录：第 {} 条消息，滚动内容与截图拼接。",
                line + 1,
                line * 17 + 3
            );
            let detail = format!(
                "Rust + DirectWrite / item-{:04X} / {:.2} ms",
                line * 31 + 11,
                (line as f32 * 1.73)
            );
            for (text, x, y, color) in [
                (
                    title,
                    24.0,
                    16.0 + line as f32 * 36.0,
                    Rgba::rgb(50, 54, 69),
                ),
                (
                    detail,
                    540.0,
                    18.0 + line as f32 * 36.0,
                    Rgba::rgb(110, 113, 133),
                ),
            ] {
                let layout = canvas
                    .text_system()
                    .layout(&text, TextStyle::sans(14.0, 500, 20.0), 480.0)
                    .map_err(|e| e.to_string())?;
                canvas.text(&layout, x, y, color);
            }
        }
        draw.finish().map_err(|e| e.to_string())
    })
    .unwrap();
    let crop = |offset: usize| Frame {
        width,
        height: viewport,
        bgra: page[offset * width as usize * 4..(offset + viewport as usize) * width as usize * 4]
            .to_vec(),
    };
    let mut stitcher = Stitcher::new(crop(0)).unwrap();
    for (offset, expected) in [
        (127, Append::Added(127)),
        (249, Append::Added(122)),
        (99, Append::Covered),
        (388, Append::Added(139)),
        (619, Append::Added(231)),
    ] {
        assert_eq!(
            stitcher.append(crop(offset)).unwrap(),
            expected,
            "offset {offset}"
        );
    }
    let result = stitcher.finish();
    assert_eq!(
        result.bgra,
        page[..result.height as usize * width as usize * 4]
    );
}

#[test]
fn upward_start_preserves_document_order_and_every_pixel() {
    let _resources = super::render::SharedScope;
    let page = document(1024, 2000);
    let viewport = 640;
    let mut stitcher = Stitcher::new(crop(&page, 600, viewport)).unwrap();
    for (offset, added) in [(487, 113), (349, 138), (192, 157), (71, 121)] {
        append_exact(&mut stitcher, crop(&page, offset, viewport), added);
    }
    let expected_height = viewport + 600 - 71;
    assert_prefix(
        &stitcher.finish(),
        &crop(&page, 71, expected_height),
        expected_height,
    );
}

#[test]
fn alternating_scroll_extends_both_ends_without_duplicate_coverage() {
    let _resources = super::render::SharedScope;
    let page = document(1024, 2000);
    let viewport = 640;
    let mut stitcher = Stitcher::new(crop(&page, 400, viewport)).unwrap();
    for (offset, expected) in [
        (288, Append::Added(112)),
        (487, Append::Added(87)),
        (169, Append::Added(119)),
        (388, Append::Covered),
        (608, Append::Added(121)),
        (346, Append::Covered),
        (83, Append::Added(86)),
    ] {
        assert_eq!(
            stitcher.append(crop(&page, offset, viewport)).unwrap(),
            expected
        );
    }
    let expected_height = viewport + 608 - 83;
    assert_prefix(
        &stitcher.finish(),
        &crop(&page, 83, expected_height),
        expected_height,
    );
}

#[test]
fn undo_upper_extension_can_resume_above_then_below() {
    let _resources = super::render::SharedScope;
    let page = document(1024, 2000);
    let viewport = 640;
    let mut stitcher = Stitcher::new(crop(&page, 500, viewport)).unwrap();
    append_exact(&mut stitcher, crop(&page, 400, viewport), 100);
    append_exact(&mut stitcher, crop(&page, 260, viewport), 140);
    assert!(stitcher.undo());
    append_exact(&mut stitcher, crop(&page, 190, viewport), 210);
    assert_eq!(
        stitcher.append(crop(&page, 380, viewport)).unwrap(),
        Append::Covered,
    );
    append_exact(&mut stitcher, crop(&page, 540, viewport), 40);
    let expected_height = viewport + 540 - 190;
    assert_prefix(
        &stitcher.finish(),
        &crop(&page, 190, expected_height),
        expected_height,
    );
}

#[test]
fn isolated_short_lines_survive_large_blank_margins() {
    let _resources = super::render::SharedScope;
    let page = rendered_page(1536, 1800, |canvas| {
        for line in 0..6 {
            text(
                canvas,
                &format!("第 {} 条：今天收到了新消息", line + 1),
                56.0,
                128.0 + line as f32 * 320.0,
                400.0,
                14.0,
            );
        }
    });
    let mut stitcher = Stitcher::new(crop(&page, 0, 720)).unwrap();
    for offset in [17, 76, 183, 303, 469, 611] {
        let result = stitcher.append(crop(&page, offset, 720)).unwrap();
        assert!(
            matches!(result, Append::Added(_) | Append::Unchanged),
            "两帧仍有相同文字，不能因全屏平均变化小而拒绝：offset={offset}, {result:?}"
        );
    }
    assert_prefix(&stitcher.finish(), &page, 720 + 611);
}

#[test]
fn smooth_scroll_one_to_four_pixels_preserves_all_rows() {
    let _resources = super::render::SharedScope;
    let page = document(960, 1100);
    let mut stitcher = Stitcher::new(crop(&page, 0, 540)).unwrap();
    let mut offset = 0;
    for step in (1..=4).cycle().take(80) {
        offset += step;
        let result = stitcher.append(crop(&page, offset, 540)).unwrap();
        assert!(
            matches!(result, Append::Added(_) | Append::Unchanged),
            "平滑滚动小步不应误判为反向或丢失重叠：offset={offset}, {result:?}"
        );
    }
    assert_prefix(&stitcher.finish(), &page, 540 + offset);
}

#[test]
fn rendered_fixed_header_and_footer_appear_once() {
    let _resources = super::render::SharedScope;
    let page = document(900, 1800);
    let header = rendered_page(900, 52, |canvas| {
        text(canvas, "团队聊天  ·  截图功能讨论", 24.0, 14.0, 820.0, 16.0);
        canvas.hline(0.0, 900.0, 51.0, Rgba::rgb(219, 223, 230));
    });
    let footer = rendered_page(900, 36, |canvas| {
        canvas.hline(0.0, 900.0, 0.0, Rgba::rgb(219, 223, 230));
        text(canvas, "已连接  ·  输入消息", 24.0, 8.0, 820.0, 12.0);
    });
    let viewport = 640;
    let content_height = viewport - header.height - footer.height;
    let framed = |offset| {
        let mut bgra = header.bgra.clone();
        bgra.extend_from_slice(&crop(&page, offset, content_height).bgra);
        bgra.extend_from_slice(&footer.bgra);
        Frame {
            width: page.width,
            height: viewport,
            bgra,
        }
    };
    let mut stitcher = Stitcher::new(framed(0)).unwrap();
    for (offset, added) in [(73, 73), (164, 91), (287, 123), (403, 116)] {
        append_exact(&mut stitcher, framed(offset), added);
    }
    let result = stitcher.finish();
    let mut expected = header.bgra;
    expected.extend_from_slice(&crop(&page, 0, content_height + 403).bgra);
    expected.extend_from_slice(&footer.bgra);
    assert_prefix(
        &result,
        &Frame {
            width: page.width,
            height: viewport + 403,
            bgra: expected,
        },
        viewport + 403,
    );
}

#[test]
fn upward_capture_keeps_fixed_header_and_footer_once() {
    let _resources = super::render::SharedScope;
    let page = document(900, 1800);
    let header = rendered_page(900, 52, |canvas| {
        text(canvas, "团队聊天  ·  截图功能讨论", 24.0, 14.0, 820.0, 16.0);
        canvas.hline(0.0, 900.0, 51.0, Rgba::rgb(219, 223, 230));
    });
    let footer = rendered_page(900, 36, |canvas| {
        canvas.hline(0.0, 900.0, 0.0, Rgba::rgb(219, 223, 230));
        text(canvas, "已连接  ·  输入消息", 24.0, 8.0, 820.0, 12.0);
    });
    let viewport = 640;
    let content_height = viewport - header.height - footer.height;
    let framed = |offset| {
        let mut bgra = header.bgra.clone();
        bgra.extend_from_slice(&crop(&page, offset, content_height).bgra);
        bgra.extend_from_slice(&footer.bgra);
        Frame {
            width: page.width,
            height: viewport,
            bgra,
        }
    };
    let mut stitcher = Stitcher::new(framed(560)).unwrap();
    for (offset, added) in [(443, 117), (293, 150), (170, 123), (71, 99)] {
        append_exact(&mut stitcher, framed(offset), added);
    }
    let expected_height = viewport + 560 - 71;
    let mut expected = header.bgra;
    expected.extend_from_slice(&crop(&page, 71, content_height + 560 - 71).bgra);
    expected.extend_from_slice(&footer.bgra);
    assert_prefix(
        &stitcher.finish(),
        &Frame {
            width: page.width,
            height: expected_height,
            bgra: expected,
        },
        expected_height,
    );
}

#[test]
fn narrow_scrolling_column_is_not_outvoted_by_static_sidebar() {
    let _resources = super::render::SharedScope;
    let body_width = 320;
    let sidebar_width = 720;
    let width = body_width + sidebar_width;
    let viewport = 640;
    let body = document(body_width, 1700);
    let sidebar = rendered_page(sidebar_width, viewport, |canvas| {
        text(canvas, "联系人与会话", 28.0, 12.0, 600.0, 18.0);
        for row in 0..12 {
            text(
                canvas,
                &format!("{}  项目讨论群  ·  当前联系人保持不动", row + 1),
                28.0,
                64.0 + row as f32 * 44.0,
                630.0,
                16.0,
            );
        }
    });
    let framed = |offset| {
        let body_view = crop(&body, offset, viewport);
        let mut bgra = Vec::with_capacity(width as usize * viewport as usize * 4);
        for (left, right) in sidebar
            .bgra
            .chunks_exact(sidebar_width as usize * 4)
            .zip(body_view.bgra.chunks_exact(body_width as usize * 4))
        {
            bgra.extend_from_slice(left);
            bgra.extend_from_slice(right);
        }
        Frame {
            width,
            height: viewport,
            bgra,
        }
    };
    let mut stitcher = Stitcher::new(framed(0)).unwrap();
    for (offset, added) in [(81, 81), (184, 103), (307, 123), (439, 132)] {
        append_exact(&mut stitcher, framed(offset), added);
    }
    let result = stitcher.finish();
    assert_eq!(result.height, viewport + 439);
    let mut extracted = Vec::with_capacity(body_width as usize * result.height as usize * 4);
    for row in result.bgra.chunks_exact(width as usize * 4) {
        extracted.extend_from_slice(&row[sidebar_width as usize * 4..]);
    }
    assert_prefix(
        &Frame {
            width: body_width,
            height: result.height,
            bgra: extracted,
        },
        &body,
        viewport + 439,
    );
}

#[test]
fn similar_list_rows_use_their_actual_number_and_time() {
    let _resources = super::render::SharedScope;
    let page = rendered_page(1040, 2200, |canvas| {
        for row in 0..44 {
            let y = 8.0 + row as f32 * 48.0;
            text(
                canvas,
                "消息发送成功：截图附件已接收。",
                32.0,
                y,
                690.0,
                14.0,
            );
            text(
                canvas,
                &format!("#{:04}   10:{:02}:{:02}", row + 1000, row / 6, row % 6 * 10),
                748.0,
                y,
                260.0,
                13.0,
            );
            canvas.hline(24.0, 1016.0, y + 34.0, Rgba::rgb(231, 233, 239));
        }
    });
    let mut stitcher = Stitcher::new(crop(&page, 0, 720)).unwrap();
    for (offset, added) in [(47, 47), (95, 48), (190, 95), (284, 94), (430, 146)] {
        append_exact(&mut stitcher, crop(&page, offset, 720), added);
    }
    assert_prefix(&stitcher.finish(), &page, 720 + 430);
}

#[test]
fn small_hover_and_caret_changes_do_not_break_alignment() {
    let _resources = super::render::SharedScope;
    let page = document(1000, 1500);
    let mut stitcher = Stitcher::new(crop(&page, 0, 640)).unwrap();
    for (index, (offset, added)) in [(73, 73), (141, 68), (237, 96)].into_iter().enumerate() {
        let mut next = crop(&page, offset, 640);
        // 改动只在重叠区，新增条带应仍与未污染文档逐像素一致。
        for y in 110..137 {
            for x in 780..862 {
                let at = (y * next.width as usize + x) * 4;
                next.bgra[at..at + 4].copy_from_slice(&[244, 238, 230, 255]);
            }
        }
        if index % 2 == 0 {
            for y in 181..199 {
                let at = (y * next.width as usize + 452) * 4;
                next.bgra[at..at + 4].copy_from_slice(&[65, 65, 65, 255]);
            }
        }
        append_exact(&mut stitcher, next, added);
    }
    assert_prefix(&stitcher.finish(), &page, 640 + 237);
}

#[test]
fn local_notification_scroll_does_not_extend_a_stationary_page() {
    let _resources = super::render::SharedScope;
    let page = document(1000, 640);
    let notification = rendered_page(300, 280, |canvas| {
        for row in 0..7 {
            text(
                canvas,
                &format!("新通知 {}：附件已上传", row + 1),
                12.0,
                9.0 + row as f32 * 38.0,
                275.0,
                14.0,
            );
        }
    });
    let with_notification = |offset: usize| {
        let mut frame = crop(&page, 0, 640);
        for y in 0..180 {
            let from = (offset + y) * 300 * 4;
            let to = ((70 + y) * 1000 + 640) * 4;
            frame.bgra[to..to + 300 * 4].copy_from_slice(&notification.bgra[from..from + 300 * 4]);
        }
        frame
    };
    let first = with_notification(0);
    let expected = first.bgra.clone();
    let mut stitcher = Stitcher::new(first).unwrap();
    let step = stitcher.append(with_notification(29)).unwrap();
    assert!(matches!(
        step,
        Append::Unchanged | Append::NoOverlap | Append::Ambiguous
    ));
    let result = stitcher.finish();
    assert_eq!(result.height, 640);
    assert!(
        result.bgra == expected,
        "局部通知滚动不能追加或改写静止页面"
    );
}

#[test]
fn visible_window_bitblt_capture_preserves_scroll_and_return() {
    visible_window_capture(
        0,
        &[
            (113, Append::Added(113)),
            (227, Append::Added(114)),
            (91, Append::Covered),
            (349, Append::Added(122)),
            (461, Append::Added(112)),
        ],
        0,
        480 + 461,
        "scroll-stitch-live.png",
    );
}

#[test]
fn visible_window_bitblt_capture_extends_upward_and_downward() {
    visible_window_capture(
        600,
        &[
            (483, Append::Added(117)),
            (337, Append::Added(146)),
            (411, Append::Covered),
            (194, Append::Added(143)),
            (387, Append::Covered),
            (551, Append::Covered),
            (706, Append::Added(106)),
            (521, Append::Covered),
            (324, Append::Covered),
            (143, Append::Added(51)),
        ],
        143,
        480 + 706 - 143,
        "scroll-stitch-live-bidirectional.png",
    );
}

#[expect(unsafe_code, reason = "仅冒烟测试创建自己拥有的 Win32 窗口")]
fn visible_window_capture(
    initial: u32,
    steps: &[(u32, Append)],
    expected_start: u32,
    expected_height: u32,
    file_name: &str,
) {
    if std::env::var_os("NCD_SCROLL_CAPTURE_SMOKE").is_none() {
        return;
    }
    use crate::native_panel::{
        gfx::Rect,
        sys::{self, Hwnd, Message, WndHandler},
    };
    use windows::{
        Win32::{
            Foundation::LRESULT,
            UI::WindowsAndMessaging::{
                CreateWindowExW, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, WM_ERASEBKGND,
                WM_PAINT, WNDCLASS_STYLES, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_POPUP,
            },
        },
        core::w,
    };
    struct Handler;
    impl WndHandler for Handler {
        fn handle(message: &Message) -> Option<LRESULT> {
            match message.id() {
                WM_PAINT => {
                    let _paint = message.hwnd().begin_paint();
                    Some(LRESULT(0))
                }
                WM_ERASEBKGND => Some(LRESULT(1)),
                _ => None,
            }
        }
    }
    struct Window(Hwnd);
    impl Drop for Window {
        fn drop(&mut self) {
            self.0.destroy();
        }
    }
    let _resources = super::render::SharedScope;
    sys::testing::per_monitor_dpi_thread();
    // SAFETY: 只查询主屏尺寸，采集前确认测试窗口全部位于屏幕内。
    let screen = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    assert!(screen.0 >= 712 && screen.1 >= 552);
    sys::register_window_class_with_style::<Handler>(
        w!("NCD.ScrollCaptureSmoke.v1"),
        WNDCLASS_STYLES(0),
    );
    // SAFETY: 独立静态窗口类，无创建参数；返回句柄立即交给同线程 Drop 守卫。
    let window = Window(Hwnd::from_raw(
        unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("NCD.ScrollCaptureSmoke.v1"),
                w!("NCD screenshot capture test"),
                WS_POPUP,
                40,
                40,
                672,
                512,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap(),
    ));
    let resources = shared().unwrap();
    let target = sys::hwnd_render_target(&resources.d2d, window.0, 672, 512, 1.0).unwrap();
    let canvas = Canvas::new(target, resources, 1.0).unwrap();
    let page = document(640, 1600);
    let bitmap = canvas
        .create_bgra_bitmap(page.width, page.height, &page.bgra)
        .unwrap();
    window.0.show_no_activate();
    sys::testing::pump_messages();
    let (left, top, _, _) = window.0.screen_rect().unwrap();
    let bounds = super::model::PixelRect {
        left: left + 16,
        top: top + 16,
        width: 640,
        height: 480,
    };
    let capture_at = |offset: u32| {
        let draw = canvas.begin_draw();
        canvas.clear(Rgba::rgb(225, 229, 235));
        canvas.draw_bitmap(
            &bitmap,
            Rect::new(16.0, 16.0, 640.0, 480.0),
            Some(Rect::new(0.0, offset as f32, 640.0, 480.0)),
            true,
        );
        draw.finish().unwrap();
        let capture = super::capture::capture_region(bounds).unwrap();
        let frame = Frame {
            width: 640,
            height: 480,
            bgra: capture.bgra,
        };
        assert_prefix(&frame, &crop(&page, offset, 480), 480);
        frame
    };
    let mut stitcher = Stitcher::new(capture_at(initial)).unwrap();
    for &(offset, expected) in steps {
        assert_eq!(stitcher.append(capture_at(offset)).unwrap(), expected);
    }
    let result = stitcher.finish();
    assert_prefix(
        &result,
        &crop(&page, expected_start, expected_height),
        expected_height,
    );
    if let Some(directory) = std::env::var_os("NCD_SCREENSHOT_SHOTS_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut rgba = result.bgra;
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        image::save_buffer(
            directory.join(file_name),
            &rgba,
            result.width,
            result.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
