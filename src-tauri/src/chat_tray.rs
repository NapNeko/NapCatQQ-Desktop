//! 头像托盘与窗口共用缓存；只有需要提醒的未读才启动闪烁。
use ncd_domain::chat_desktop::{ChatAccountStatus, ChatTrayNotification};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Manager,
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const PULSE_LEVELS: [f64; 2] = [1.0, 0.22];
struct Avatar {
    revision: [u8; 32],
    window: Image<'static>,
    online: Image<'static>,
    offline: Image<'static>,
    badge: Image<'static>,
    offline_badge: Image<'static>,
    pulse: Vec<Image<'static>>,
    offline_pulse: Vec<Image<'static>>,
}
impl Avatar {
    fn new(window: Image<'static>) -> Self {
        let buffer =
            image::RgbaImage::from_raw(window.width(), window.height(), window.rgba().to_vec());
        let online = buffer
            .map(|pixels| {
                let small =
                    image::imageops::resize(&pixels, 32, 32, image::imageops::FilterType::Lanczos3);
                rounded_avatar(Image::new_owned(small.into_raw(), 32, 32))
            })
            .unwrap_or_else(|| Image::new_owned(vec![0; 32 * 32 * 4], 32, 32));
        let offline = decorated(&online, false, true);
        let pulse = PULSE_LEVELS
            .iter()
            .map(|level| flash_frame(&online, *level))
            .collect();
        let offline_pulse = PULSE_LEVELS
            .iter()
            .map(|level| flash_frame(&offline, *level))
            .collect();
        Self {
            revision: Sha256::digest(window.rgba()).into(),
            offline,
            badge: decorated(&online, true, false),
            offline_badge: decorated(&online, true, true),
            pulse,
            offline_pulse,
            online,
            window,
        }
    }
    fn tray(&self, offline: bool, badge: bool) -> Image<'static> {
        match (offline, badge) {
            (false, false) => &self.online,
            (true, false) => &self.offline,
            (false, true) => &self.badge,
            (true, true) => &self.offline_badge,
        }
        .clone()
    }
    fn pulse_frame(&self, offline: bool, phase: usize) -> Image<'static> {
        let frames = if offline {
            &self.offline_pulse
        } else {
            &self.pulse
        };
        frames[phase % PULSE_LEVELS.len()].clone()
    }
}
#[derive(PartialEq)]
struct Appearance {
    unread: bool,
    offline: bool,
    background: bool,
    ready: bool,
    avatar_revision: [u8; 32],
    mode: ChatTrayNotification,
    tooltip: String,
    qq: String,
}
#[derive(Clone)]
struct CachedAvatar {
    image: Option<Arc<Avatar>>,
    refresh_at: Instant,
}
pub struct ChatTrayState {
    ids: Mutex<HashSet<String>>,
    avatars: Mutex<HashMap<String, CachedAvatar>>,
    fallback: Mutex<Option<Arc<Avatar>>>,
    displayed: Mutex<HashMap<String, Appearance>>,
    window_displayed: Mutex<Option<(String, bool, [u8; 32], String)>>,
    loading: Mutex<HashSet<String>>,
    phase: Mutex<usize>,
}
impl Default for ChatTrayState {
    fn default() -> Self {
        Self {
            ids: Mutex::new(HashSet::new()),
            avatars: Mutex::new(HashMap::new()),
            fallback: Mutex::new(None),
            displayed: Mutex::new(HashMap::new()),
            window_displayed: Mutex::new(None),
            loading: Mutex::new(HashSet::new()),
            phase: Mutex::new(0),
        }
    }
}
fn tray_id(bot: &str, qq: &str) -> String {
    format!(
        "chat-{}",
        hex::encode(Sha256::digest(format!("{bot}/{qq}").as_bytes()))
    )
}
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}
fn fallback(app: &AppHandle) -> Arc<Avatar> {
    let registry = app.state::<ChatTrayState>();
    let mut cached = lock(&registry.fallback);
    Arc::clone(cached.get_or_insert_with(|| {
        Arc::new(Avatar::new(
            crate::window_icon::main_window_icon(app)
                .unwrap_or_else(|_| Image::new_owned(vec![0; 32 * 32 * 4], 32, 32)),
        ))
    }))
}

async fn avatar(app: &AppHandle, qq: &str) -> Option<Arc<Avatar>> {
    let key = crate::avatar_cache::AvatarKey::user(qq)?;
    let pixels = crate::avatar_cache::load(app, &key).await?;
    let previous = lock(&app.state::<ChatTrayState>().avatars)
        .get(qq)
        .and_then(|entry| entry.image.clone());
    let revision: [u8; 32] = Sha256::digest(&pixels.rgba).into();
    if let Some(previous) = previous.filter(|image| image.revision == revision) {
        return Some(previous);
    }
    Some(Arc::new(Avatar::new(Image::new_owned(
        pixels.rgba.clone(),
        pixels.size,
        pixels.size,
    ))))
}
fn current_avatar(app: &AppHandle, qq: &str) -> (Arc<Avatar>, bool) {
    let registry = app.state::<ChatTrayState>();
    let cached = lock(&registry.avatars).get(qq).cloned();
    let previous = cached.as_ref().and_then(|entry| entry.image.clone());
    if cached.is_some_and(|entry| entry.refresh_at > Instant::now()) {
        let ready = previous.is_some();
        return (previous.unwrap_or_else(|| fallback(app)), ready);
    }
    let mut loading = lock(&registry.loading);
    if loading.len() < 64 && loading.insert(qq.into()) {
        let app = app.clone();
        let qq = qq.to_owned();
        tauri::async_runtime::spawn(async move {
            let image = avatar(&app, &qq).await;
            let retry = crate::avatar_cache::AvatarKey::user(&qq)
                .map(|key| crate::avatar_cache::refresh_after(&key))
                .unwrap_or(crate::avatar_cache::FAILURE_TTL);
            let registry = app.state::<ChatTrayState>();
            {
                let mut avatars = lock(&registry.avatars);
                let image =
                    image.or_else(|| avatars.get(&qq).and_then(|entry| entry.image.clone()));
                if avatars.len() < 64 || avatars.contains_key(&qq) {
                    avatars.insert(
                        qq.clone(),
                        CachedAvatar {
                            image,
                            refresh_at: Instant::now() + retry,
                        },
                    );
                }
            }
            lock(&registry.loading).remove(&qq);
            app.state::<crate::AppState>().chat.notify_changed();
        });
    }
    let ready = previous.is_some();
    (previous.unwrap_or_else(|| fallback(app)), ready)
}
fn decorated(icon: &Image<'_>, unread: bool, offline: bool) -> Image<'static> {
    notification_frame(icon, unread, offline, 1.0)
}
fn rounded_avatar(icon: Image<'static>) -> Image<'static> {
    let mut pixels = icon.rgba().to_vec();
    let size = f64::from(icon.width());
    let radius = size * 0.20;
    for y in 0..icon.height() {
        for x in 0..icon.width() {
            let dx = (f64::from(x) + 0.5 - size / 2.0).abs() - (size / 2.0 - radius);
            let dy = (f64::from(y) + 0.5 - size / 2.0).abs() - (size / 2.0 - radius);
            let coverage = (radius + 0.5 - dx.max(0.0).hypot(dy.max(0.0))).clamp(0.0, 1.0);
            let alpha = ((y * icon.width() + x) * 4 + 3) as usize;
            pixels[alpha] = (f64::from(pixels[alpha]) * coverage).round() as u8;
        }
    }
    Image::new_owned(pixels, icon.width(), icon.height())
}
fn flash_frame(icon: &Image<'_>, strength: f64) -> Image<'static> {
    if strength >= 1.0 {
        return decorated(icon, true, false);
    }
    let mut pixels = icon.rgba().to_vec();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = (f64::from(pixel[3]) * strength).round() as u8;
    }
    Image::new_owned(pixels, icon.width(), icon.height())
}
fn notification_frame(
    icon: &Image<'_>,
    unread: bool,
    offline: bool,
    strength: f64,
) -> Image<'static> {
    let mut pixels = icon.rgba().to_vec();
    let width = icon.width();
    let height = icon.height();
    for y in 0..height {
        for x in 0..width {
            let offset = ((y * width + x) * 4) as usize;
            if offline {
                let gray = (u32::from(pixels[offset])
                    + u32::from(pixels[offset + 1])
                    + u32::from(pixels[offset + 2]))
                    / 3;
                pixels[offset..offset + 3].fill(gray as u8);
            }
            let dx = x as f64 - width as f64 * 0.80;
            let dy = y as f64 - height as f64 * 0.80;
            let distance = dx.hypot(dy);
            let coverage = (width as f64 * 0.18 + 0.5 - distance).clamp(0.0, 1.0);
            if unread && coverage > 0.0 {
                let color = if distance <= width as f64 * 0.135 {
                    [242.0, 76.0, 94.0]
                } else {
                    [255.0; 3]
                };
                let overlay = coverage * strength;
                let base = f64::from(pixels[offset + 3]) / 255.0 * (1.0 - overlay);
                let alpha = overlay + base;
                for channel in 0..3 {
                    pixels[offset + channel] = ((color[channel] * overlay
                        + f64::from(pixels[offset + channel]) * base)
                        / alpha)
                        .round() as u8;
                }
                pixels[offset + 3] = (alpha * 255.0).round() as u8;
            }
        }
    }
    Image::new_owned(pixels, width, height)
}

async fn create(
    app: &AppHandle,
    id: &str,
    row: &ChatAccountStatus,
    icon: Image<'static>,
    tooltip: &str,
) -> Result<(), String> {
    let bot = row.target.bot_id.clone();
    let qq = row.preference.self_id.clone();
    let builder = TrayIconBuilder::with_id(id)
        .icon(icon.clone())
        .tooltip(tooltip);
    #[cfg(target_os = "linux")]
    let builder = {
        let bot = bot.clone();
        let qq = qq.clone();
        let open_id = format!("{id}:open-chat");
        let panel_id = format!("{id}:account-panel");
        let preview_id = format!("{id}:unread-preview");
        builder
            .menu(
                &tauri::menu::MenuBuilder::new(app)
                    .text(&open_id, "打开聊天")
                    .text(&panel_id, "账号面板")
                    .text(&preview_id, "未读预览")
                    .build()
                    .map_err(|e| e.to_string())?,
            )
            .show_menu_on_left_click(true)
            .on_menu_event(move |app, event| {
                let action = event.id().as_ref();
                if action == open_id {
                    let (app, bot, qq) = (app.clone(), bot.clone(), qq.clone());
                    tauri::async_runtime::spawn(async move {
                        crate::chat_tray_panel_native::hide(&app);
                        if let Err(error) =
                            crate::chat_window::open_from_tray(app, bot, qq, None).await
                        {
                            tracing::warn!(%error, "聊天托盘操作失败");
                        }
                    });
                } else if action == panel_id || action == preview_id {
                    let menu = action == panel_id;
                    let (panel_app, bot, qq) = (app.clone(), bot.clone(), qq.clone());
                    if let Err(error) = app.run_on_main_thread(move || {
                        let anchor = crate::native_panel::window::menu_anchor();
                        crate::chat_tray_panel_native::show(&panel_app, bot, qq, anchor, menu);
                    }) {
                        tracing::warn!(%error, "打开聊天托盘面板失败");
                    }
                }
            })
    };
    #[cfg(not(target_os = "linux"))]
    let builder = builder.show_menu_on_left_click(false);
    let tray = builder
        .on_tray_icon_event(move |tray, event| {
            let app = tray.app_handle().clone();
            match event {
                #[cfg(windows)]
                TrayIconEvent::Enter { position, rect, .. }
                | TrayIconEvent::Move { position, rect, .. } => {
                    crate::chat_tray_panel_native::hover(
                        &app,
                        bot.clone(),
                        qq.clone(),
                        tray_anchor(&rect, position),
                    );
                }
                #[cfg(windows)]
                TrayIconEvent::Leave { .. } => {
                    crate::chat_tray_panel_native::cancel_hover(&app, bot.clone(), qq.clone());
                }
                #[cfg(windows)]
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    position,
                    rect,
                    ..
                } => {
                    let bot = bot.clone();
                    let qq = qq.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::chat_tray_panel_native::show(
                            &app,
                            bot,
                            qq,
                            tray_anchor(&rect, position),
                            true,
                        );
                    });
                }
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    let bot = bot.clone();
                    let qq = qq.clone();
                    tauri::async_runtime::spawn(async move {
                        #[cfg(any(windows, target_os = "linux"))]
                        crate::chat_tray_panel_native::hide(&app);
                        if let Err(e) = crate::chat_window::open_from_tray(app, bot, qq, None).await
                        {
                            tracing::warn!("chat tray action: {e}");
                        }
                    });
                }
                _ => {}
            }
        })
        .build(app)
        .map_err(|e| e.to_string())?;
    let _ = tray;
    Ok(())
}

#[cfg(windows)]
fn tray_anchor(rect: &tauri::Rect, position: tauri::PhysicalPosition<f64>) -> (i32, i32) {
    let scale =
        crate::windows_ui::monitor_dpi_scale_at(position.x as i32, position.y as i32).max(0.25);
    let origin = rect.position.to_physical::<i32>(scale);
    let size = rect.size.to_physical::<u32>(scale);
    if size.width == 0 || size.height == 0 {
        (position.x as i32, position.y as i32)
    } else {
        (
            origin.x + size.width as i32 / 2,
            origin.y + size.height as i32 / 2,
        )
    }
}

fn update_window(app: &AppHandle, row: Option<&ChatAccountStatus>) -> Result<(), String> {
    let registry = app.state::<ChatTrayState>();
    let Some(window) = app.get_webview_window(crate::chat_window::CHAT_WINDOW_LABEL) else {
        *lock(&registry.window_displayed) = None;
        return Ok(());
    };
    let Some(row) = row else {
        return Ok(());
    };
    let (avatar, ready) = current_avatar(app, &row.preference.self_id);
    let title = format!("{} ({}) · 聊天", row.target.name, row.preference.self_id);
    let identity = tray_id(&row.target.bot_id, &row.preference.self_id);
    let appearance = (identity.clone(), ready, avatar.revision, title.clone());
    if lock(&registry.window_displayed).as_ref() == Some(&appearance) {
        return Ok(());
    }
    window
        .set_icon(avatar.window.clone())
        .map_err(|e| e.to_string())?;
    window.set_title(&title).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        crate::window_icon::set_taskbar_identity(
            windows::Win32::Foundation::HWND(hwnd.0),
            &format!("{}.Chat.{}", app.config().identifier, identity),
        )
        .map_err(|e| e.to_string())?;
    }
    *lock(&registry.window_displayed) = Some(appearance);
    Ok(())
}
async fn refresh(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let status = state.chat.desktop_status().await;
    let selected = state.chat.view().selected_bot;
    let window_row = status
        .accounts
        .iter()
        .find(|r| selected.as_ref() == Some(&r.target.bot_id));
    let rows: Vec<_> = status
        .accounts
        .iter()
        .filter(|r| r.preference.enabled && r.preference.tray)
        .collect();
    let wanted: HashSet<_> = rows
        .iter()
        .map(|r| tray_id(&r.target.bot_id, &r.preference.self_id))
        .collect();
    let registry = app.state::<ChatTrayState>();
    let mut qqs: HashSet<_> = rows.iter().map(|r| r.preference.self_id.as_str()).collect();
    if app
        .get_webview_window(crate::chat_window::CHAT_WINDOW_LABEL)
        .is_some()
        && let Some(row) = window_row
    {
        qqs.insert(&row.preference.self_id);
    }
    lock(&registry.avatars).retain(|qq, _| qqs.contains(qq.as_str()));
    let remove: Vec<_> = lock(&registry.ids).difference(&wanted).cloned().collect();
    for id in remove {
        app.remove_tray_by_id(&id);
        lock(&registry.ids).remove(&id);
        lock(&registry.displayed).remove(&id);
    }
    update_window(app, window_row)?;
    for row in rows {
        let id = tray_id(&row.target.bot_id, &row.preference.self_id);
        let offline = !row.target.running || row.target.online == Some(false);
        let state = if offline {
            "离线"
        } else if !row.preference.background {
            "按需聊天"
        } else {
            match &row.connection {
                ncd_domain::onebot_debug::DebugReceiverState::Connected => "后台接收",
                ncd_domain::onebot_debug::DebugReceiverState::Connecting => "正在连接",
                ncd_domain::onebot_debug::DebugReceiverState::Reconnecting { .. } => "正在重连",
                ncd_domain::onebot_debug::DebugReceiverState::Stopped { .. } => "接收已停止",
            }
        };
        let tooltip = format!(
            "{} ({}) · {} · {} 条未读",
            row.target.name, row.preference.self_id, state, row.unread
        );
        let (avatar, ready) = current_avatar(app, &row.preference.self_id);
        let appearance = Appearance {
            unread: row.notification_unread > 0,
            offline,
            background: row.preference.background,
            ready,
            avatar_revision: avatar.revision,
            mode: row.preference.tray_notification,
            tooltip: tooltip.clone(),
            qq: row.preference.self_id.clone(),
        };
        if lock(&registry.displayed).get(&id) == Some(&appearance) {
            continue;
        }
        let icon = if appearance.unread && appearance.mode == ChatTrayNotification::Flash {
            avatar.pulse_frame(offline, *lock(&registry.phase))
        } else {
            avatar.tray(
                offline,
                appearance.unread && appearance.mode == ChatTrayNotification::Badge,
            )
        };
        if app.tray_by_id(&id).is_none() {
            create(app, &id, row, icon.clone(), &tooltip).await?;
            lock(&registry.ids).insert(id.clone());
        }
        if let Some(tray) = app.tray_by_id(&id) {
            tray.set_icon(Some(icon)).map_err(|e| e.to_string())?;
            tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())?;
        }
        lock(&registry.displayed).insert(id, appearance);
    }
    Ok(())
}
fn has_flashing(app: &AppHandle) -> bool {
    lock(&app.state::<ChatTrayState>().displayed)
        .values()
        .any(|state| state.unread && state.mode == ChatTrayNotification::Flash)
}
fn pulse(app: &AppHandle) {
    let registry = app.state::<ChatTrayState>();
    let phase = {
        let mut phase = lock(&registry.phase);
        *phase = (*phase + 1) % PULSE_LEVELS.len();
        *phase
    };
    let rows: Vec<_> = lock(&registry.displayed)
        .iter()
        .filter(|(_, state)| state.unread && state.mode == ChatTrayNotification::Flash)
        .map(|(id, state)| (id.clone(), state.qq.clone(), state.offline))
        .collect();
    for (id, qq, offline) in rows {
        let avatar = lock(&registry.avatars)
            .get(&qq)
            .and_then(|entry| entry.image.clone())
            .unwrap_or_else(|| fallback(app));
        if let Some(tray) = app.tray_by_id(&id) {
            let icon = avatar.pulse_frame(offline, phase);
            if let Err(e) = tray.set_icon(Some(icon)) {
                tracing::warn!("chat tray pulse: {e}");
            }
        }
    }
}
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<crate::AppState>();
        let period = Duration::from_millis(600);
        // 消息刷新不能重新开始计时，忙碌会话也需要持续闪烁。
        let mut pulse_timer =
            tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        pulse_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut avatar_timer = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(30),
            Duration::from_secs(30),
        );
        avatar_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if let Err(e) = refresh(&app).await {
                tracing::warn!("chat tray: {e}");
            }
            #[cfg(any(windows, target_os = "linux"))]
            crate::chat_tray_panel_native::refresh_if_visible(&app);
            loop {
                tokio::select! {
                    _ = state.chat.changed() => {
                        tokio::time::sleep(Duration::from_millis(200)).await; break;
                    }
                    _ = avatar_timer.tick() => {
                        #[cfg(any(windows, target_os = "linux"))]
                        crate::tray_panel_native::refresh_if_visible(&app);
                        break;
                    }
                    _ = pulse_timer.tick(), if has_flashing(&app) => pulse(&app),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn tray_anchor_is_independent_of_cursor_movement_inside_the_icon() {
        let rect = tauri::Rect {
            position: tauri::PhysicalPosition::new(100, 200).into(),
            size: tauri::PhysicalSize::new(32, 32).into(),
        };
        assert_eq!(
            tray_anchor(&rect, tauri::PhysicalPosition::new(101.0, 202.0)),
            (116, 216)
        );
        assert_eq!(
            tray_anchor(&rect, tauri::PhysicalPosition::new(130.0, 230.0)),
            (116, 216)
        );
    }
    #[test]
    fn unread_marker_is_red_in_the_lower_right_and_offline_stays_gray() {
        let original = Image::new_owned([120, 70, 20, 255].repeat(32 * 32), 32, 32);
        let badge = decorated(&original, true, true);
        let pixel = |x: usize, y: usize| &badge.rgba()[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(pixel(25, 25), &[242, 76, 94, 255]);
        assert_eq!(pixel(25, 5), &[70, 70, 70, 255]);
        assert_eq!(original.rgba()[0..4], [120, 70, 20, 255]);
    }
    #[test]
    fn avatar_prepares_window_and_tray_sizes_without_reprocessing_each_flash() {
        let avatar = Avatar::new(Image::new_owned(
            [120, 70, 20, 255].repeat(128 * 128),
            128,
            128,
        ));
        assert_eq!((avatar.window.width(), avatar.window.height()), (128, 128));
        for icon in [
            &avatar.online,
            &avatar.offline,
            &avatar.badge,
            &avatar.offline_badge,
        ] {
            assert_eq!((icon.width(), icon.height()), (32, 32));
        }
    }
    #[test]
    fn changed_avatar_pixels_invalidate_tray_and_window_appearances() {
        let first = Avatar::new(Image::new_owned(
            [10, 20, 30, 255].repeat(128 * 128),
            128,
            128,
        ));
        let changed = Avatar::new(Image::new_owned(
            [80, 20, 30, 255].repeat(128 * 128),
            128,
            128,
        ));
        let appearance = |avatar: &Avatar| Appearance {
            unread: false,
            offline: false,
            background: true,
            ready: true,
            avatar_revision: avatar.revision,
            mode: ChatTrayNotification::Badge,
            tooltip: "account".into(),
            qq: "950001".into(),
        };
        assert!(appearance(&first) != appearance(&changed));
        assert_ne!(
            ("account", true, first.revision, "title"),
            ("account", true, changed.revision, "title"),
        );
    }
    #[test]
    fn flash_changes_avatar_opacity_without_losing_its_identity() {
        let avatar = Avatar::new(Image::new_owned(
            [120, 70, 20, 255].repeat(128 * 128),
            128,
            128,
        ));
        let bright = avatar.pulse_frame(false, 0);
        let dim = avatar.pulse_frame(false, 1);
        assert_ne!(bright.rgba(), dim.rgba());
        for (index, original) in avatar.online.rgba().chunks_exact(4).enumerate() {
            let x = index % 32;
            let y = index / 32;
            if x < 20 || y < 20 {
                assert_eq!(&bright.rgba()[index * 4..index * 4 + 4], original);
                assert_eq!(&dim.rgba()[index * 4..index * 4 + 3], &original[..3]);
                assert_eq!(
                    dim.rgba()[index * 4 + 3],
                    (f64::from(original[3]) * 0.22).round() as u8
                );
            }
        }
        assert_eq!(avatar.online.rgba()[3], 0);
        assert_eq!(avatar.pulse_frame(true, 1).rgba()[..3], [70, 70, 70]);
    }
}
