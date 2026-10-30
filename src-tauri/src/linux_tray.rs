//! XEmbed 兼容托盘：嵌入成功后接管点击，未嵌入时保留 Tauri 的系统菜单。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gdk;
use gtk::gdk_pixbuf::{Colorspace, Pixbuf};
use gtk::glib::{
    self,
    translate::{ToGlibPtr, from_glib, from_glib_full},
};
use gtk::prelude::*;
use tauri::AppHandle;
use tauri::image::Image;
use tauri::tray::{MouseButton, TrayIcon};

thread_local! {
    static TRAYS: RefCell<HashMap<String, Rc<CompatTray>>> = RefCell::new(HashMap::new());
}

/// 坐标属于图标所在 X11 display 的逻辑像素，display 仅在 GTK 主线程使用。
pub struct Click {
    pub button: MouseButton,
    pub anchor: (i32, i32),
    pub display: gdk::Display,
}

struct CompatTray {
    icon: StatusIcon,
    system_tray: TrayIcon,
    display: gdk::Display,
    click: Box<dyn Fn(Click)>,
}

impl CompatTray {
    fn sync_backend(&self) {
        let embedded = self.icon.is_embedded();
        if let Err(error) = self.system_tray.set_visible(!embedded) {
            tracing::warn!(tray = self.system_tray.id().as_ref(), %error, "切换 Linux 托盘显示失败");
        }
        if embedded {
            crate::native_panel::window::set_tray_display(Some(&self.display));
        }
    }

    fn click(&self, button: MouseButton, anchor: (i32, i32)) {
        (self.click)(Click {
            button,
            anchor,
            display: self.display.clone(),
        });
    }

    fn keyboard_anchor(&self) -> (i32, i32) {
        self.icon.anchor().unwrap_or_else(|| {
            self.display
                .default_seat()
                .and_then(|seat| seat.pointer())
                .map(|pointer| {
                    let (_, x, y) = pointer.position();
                    (x, y)
                })
                .unwrap_or((0, 0))
        })
    }
}

/// 保留原有 SNI 托盘作为回退；GTK 的 embedded 通知决定哪个图标可见。
pub fn attach(
    app: &AppHandle,
    tray: &TrayIcon,
    icon: Image<'static>,
    tooltip: &str,
    click: impl Fn(Click) + Send + 'static,
) -> Result<(), String> {
    let tray = tray.clone();
    let tooltip = tooltip.to_owned();
    app.run_on_main_thread(move || {
        let Some(display) = x11_display() else {
            return;
        };
        let status_icon = {
            // GtkStatusIcon 只在默认 display 为 X11 时创建底层 GtkPlug。
            // 同步创建后立即恢复，后续 WebView 与普通窗口继续使用原来的 display。
            let _context = IconCreation::new(&display, tray.id().as_ref());
            StatusIcon::new(&pixbuf(&icon))
        };
        status_icon.set_tooltip(&tooltip);
        let data = Rc::new(CompatTray {
            icon: status_icon,
            system_tray: tray,
            display,
            click: Box::new(click),
        });
        let weak = Rc::downgrade(&data);
        data.icon
            .0
            .connect_notify_local(Some("embedded"), move |_, _| {
                if let Some(data) = weak.upgrade() {
                    data.sync_backend();
                }
            });
        // GtkStatusIcon 默认在按下时激活/弹菜单；吞掉已处理按钮的默认动作，
        // 统一在松开后打开面板，避免宿主仍持有按键时争夺焦点。
        data.icon
            .0
            .connect_local("button-press-event", false, |values| {
                Some(mouse_click(values).is_some().to_value())
            });
        let weak = Rc::downgrade(&data);
        data.icon
            .0
            .connect_local("button-release-event", false, move |values| {
                let click = mouse_click(values);
                if let (Some(data), Some((button, anchor))) = (weak.upgrade(), click) {
                    data.click(button, anchor);
                }
                Some(click.is_some().to_value())
            });
        // 键盘激活和键盘上下文菜单继续沿用 GTK 对应信号。
        for (signal, button) in [
            ("activate", MouseButton::Left),
            ("popup-menu", MouseButton::Right),
        ] {
            let weak = Rc::downgrade(&data);
            data.icon.0.connect_local(signal, false, move |_| {
                if let Some(data) = weak.upgrade() {
                    data.click(button, data.keyboard_anchor());
                }
                None
            });
        }
        data.sync_backend();
        TRAYS.with(|trays| {
            trays
                .borrow_mut()
                .insert(data.system_tray.id().as_ref().to_owned(), data);
        });
    })
    .map_err(|error| error.to_string())
}

/// 与 SNI 图标同步更新，包括账号未读标记和闪烁帧。
pub fn set_icon(app: &AppHandle, id: &str, icon: &Image<'_>) -> Result<(), String> {
    let id = id.to_owned();
    let icon = Image::new_owned(icon.rgba().to_vec(), icon.width(), icon.height());
    app.run_on_main_thread(move || {
        TRAYS.with(|trays| {
            if let Some(tray) = trays.borrow().get(&id) {
                tray.icon.set_pixbuf(&pixbuf(&icon));
            }
        });
    })
    .map_err(|error| error.to_string())
}

pub fn set_tooltip(app: &AppHandle, id: &str, tooltip: &str) -> Result<(), String> {
    let id = id.to_owned();
    let tooltip = tooltip.to_owned();
    app.run_on_main_thread(move || {
        TRAYS.with(|trays| {
            if let Some(tray) = trays.borrow().get(&id) {
                tray.icon.set_tooltip(&tooltip);
            }
        });
    })
    .map_err(|error| error.to_string())
}

/// 与 Tauri 托盘移除一起调用，释放 GTK 图标及其弱引用回调。
pub fn remove(app: &AppHandle, id: &str) -> Result<(), String> {
    let id = id.to_owned();
    app.run_on_main_thread(move || {
        TRAYS.with(|trays| {
            trays.borrow_mut().remove(&id);
        });
    })
    .map_err(|error| error.to_string())
}

fn x11_display() -> Option<gdk::Display> {
    let manager = gdk::DisplayManager::get();
    manager
        .list_displays()
        .into_iter()
        .find(|display| !display.is_closed() && display.type_().name() == "GdkX11Display")
        .or_else(|| {
            let name = std::env::var("DISPLAY").ok()?;
            gdk::Display::open(&name).filter(|display| display.type_().name() == "GdkX11Display")
        })
}

struct IconCreation {
    manager: gdk::DisplayManager,
    previous_display: Option<gdk::Display>,
    previous_class: Option<glib::GString>,
}

impl IconCreation {
    fn new(display: &gdk::Display, class: &str) -> Self {
        let manager = gdk::DisplayManager::get();
        let previous_display = manager.default_display();
        let previous_class = gdk::program_class();
        manager.set_default_display(display);
        // GtkWindow 在初始化时复制 program_class；第一次映射起就使用独立
        // WM_CLASS，避免 set_name 对已实现 GtkPlug 的销毁和重建。
        gdk::set_program_class(class);
        Self {
            manager,
            previous_display,
            previous_class,
        }
    }
}

impl Drop for IconCreation {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous_class {
            gdk::set_program_class(previous);
        }
        if let Some(previous) = &self.previous_display {
            self.manager.set_default_display(previous);
        }
    }
}

fn pixbuf(image: &Image<'_>) -> Pixbuf {
    Pixbuf::from_bytes(
        &glib::Bytes::from_owned(image.rgba().to_vec()),
        Colorspace::Rgb,
        true,
        8,
        image.width() as i32,
        image.height() as i32,
        image.width() as i32 * 4,
    )
}

fn mouse_click(values: &[glib::Value]) -> Option<(MouseButton, (i32, i32))> {
    let event = values.get(1)?.get::<gdk::Event>().ok()?;
    let event = event.downcast_ref::<gdk::EventButton>()?;
    let button = match event.button() {
        1 => MouseButton::Left,
        3 => MouseButton::Right,
        _ => return None,
    };
    let (x, y) = event.root();
    Some((button, (x.round() as i32, y.round() as i32)))
}

// gtk-rs 不再提供已弃用的高层 StatusIcon；GTK 3 的 ABI 仍支持此兼容协议。
struct StatusIcon(glib::Object);

#[expect(unsafe_code, reason = "GTK 3 GtkStatusIcon 的薄 FFI 封装")]
impl StatusIcon {
    fn new(pixbuf: &Pixbuf) -> Self {
        // SAFETY: GTK 已初始化且位于主线程，构造函数返回一份完整 GObject 引用。
        let object = unsafe {
            from_glib_full(
                gtk::ffi::gtk_status_icon_new_from_pixbuf(pixbuf.to_glib_none().0).cast(),
            )
        };
        Self(object)
    }

    fn pointer(&self) -> *mut gtk::ffi::GtkStatusIcon {
        self.0.as_ptr().cast()
    }

    fn is_embedded(&self) -> bool {
        // SAFETY: 对象由 GtkStatusIcon 构造，当前线程持有其完整引用。
        unsafe { from_glib(gtk::ffi::gtk_status_icon_is_embedded(self.pointer())) }
    }

    fn anchor(&self) -> Option<(i32, i32)> {
        let mut area = gdk::ffi::GdkRectangle {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        // SAFETY: 当前为 X11 StatusIcon；GTK 在有效图标窗口存在时填写 area，
        // screen 与 orientation 为该 API 明确允许省略的输出参数。
        let available: bool = unsafe {
            from_glib(gtk::ffi::gtk_status_icon_get_geometry(
                self.pointer(),
                std::ptr::null_mut(),
                &mut area,
                std::ptr::null_mut(),
            ))
        };
        available.then_some((area.x + area.width / 2, area.y + area.height / 2))
    }

    fn set_pixbuf(&self, pixbuf: &Pixbuf) {
        // SAFETY: GTK 只读取有效 Pixbuf 并自行保留需要的引用。
        unsafe {
            gtk::ffi::gtk_status_icon_set_from_pixbuf(self.pointer(), pixbuf.to_glib_none().0);
        }
    }

    fn set_tooltip(&self, tooltip: &str) {
        self.0.set_property("title", tooltip);
        self.0.set_property("tooltip-text", tooltip);
    }
}

#[expect(unsafe_code, reason = "销毁 XEmbed 图标前解除 GTK 可见状态")]
impl Drop for StatusIcon {
    fn drop(&mut self) {
        // SAFETY: GObject 引用尚未释放；先取消可见状态，GTK 再随对象析构移除图标。
        unsafe { gtk::ffi::gtk_status_icon_set_visible(self.pointer(), 0) }
    }
}
