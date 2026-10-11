mod config;
mod hotkey;
mod keymap;
mod store;
mod theme;
mod tray;
mod ui;

use config::Config;
use gpui_kit::*;
use store::Store;
use ui::RootView;

/// 跨模块共享的应用状态
pub struct Shared {
    pub window: Option<AnyWindowHandle>,
    pub root: Option<WeakEntity<RootView>>,
}

impl Global for Shared {}

pub fn shared_root(cx: &App) -> Option<Entity<RootView>> {
    cx.global::<Shared>()
        .root
        .as_ref()
        .and_then(|r| r.upgrade())
}

/// 保存当前笔记并隐藏整个应用
pub fn hide_window(cx: &mut App) {
    if let Some(root) = shared_root(cx) {
        root.update(cx, |r, cx| r.flush_save(cx));
    }
    request_hide(cx);
}

static HIDE_REQUESTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 请求隐藏应用;headless 测试平台没有真实窗口可隐藏,只记录请求
pub fn request_hide(cx: &mut App) {
    HIDE_REQUESTED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if !cfg!(test) {
        cx.hide();
    }
}

#[cfg(test)]
pub fn hide_count() -> u64 {
    HIDE_REQUESTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// 显示并聚焦窗口,焦点进编辑器
pub fn show_window(cx: &mut App) {
    let Some(handle) = cx.global::<Shared>().window else {
        return;
    };
    let _ = cx.update_window(handle, |_, window, cx| {
        move_window_to_mouse_screen(window);
        activate_app();
        window.activate_window();
        if let Some(root) = shared_root(cx) {
            root.update(cx, |r, cx| r.focus_editor(window, cx));
        }
    });
}

/// 显示窗口并新建一条笔记
pub fn new_note(cx: &mut App) {
    let Some(handle) = cx.global::<Shared>().window else {
        return;
    };
    let _ = cx.update_window(handle, |_, window, cx| {
        activate_app();
        window.activate_window();
        if let Some(root) = shared_root(cx) {
            root.update(cx, |r, cx| r.create_note(window, cx));
        }
    });
}

/// 在 Finder 中打开笔记目录
pub fn open_storage(cx: &mut App) {
    let dir = cx.global::<Config>().notes_dir.clone();
    std::process::Command::new("open")
        .arg(dir)
        .spawn()
        .expect("open notes dir");
}

/// 开关式唤起:聚焦则隐藏,否则显示
pub fn toggle_window(cx: &mut App) {
    let Some(handle) = cx.global::<Shared>().window else {
        return;
    };
    let active = cx
        .update_window(handle, |_, window, _| window.is_window_active())
        .unwrap_or(false);
    if active {
        hide_window(cx);
    } else {
        show_window(cx);
    }
}

fn main() {
    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(|cx| {
        gpui_kit::init(cx);
        theme::apply(cx);
        keymap::bind(cx);
        disable_titlebar_double_click();

        let config = Config::load();
        std::fs::create_dir_all(&config.notes_dir).expect("create notes dir");
        let store = Store::new(config.notes_dir.clone());
        cx.set_global(config.clone());

        let (window, root) = gpui_kit::open_window(window_options(cx), cx, |window, cx| {
            hide_traffic_lights(window);
            let view = cx.new(|cx| RootView::new(store, window, cx));
            window.on_window_should_close(cx, |_, cx| {
                hide_window(cx);
                false
            });
            view
        })
        .expect("open window");

        cx.set_global(Shared {
            window: Some(window),
            root: Some(root.downgrade()),
        });

        hotkey::install(cx, &config.hotkey);
        tray::install(cx);
        set_accessory();
        activate_app();
    });
}

fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(880.), px(560.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("ePostit".into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_min_size: Some(size(px(600.), px(400.))),
        kind: WindowKind::Normal,
        is_movable: true,
        is_resizable: true,
        ..Default::default()
    }
}

/// 激活应用(菜单栏应用唤起窗口的强制激活路径)
#[cfg(target_os = "macos")]
pub fn activate_app() {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    NSRunningApplication::currentApplication()
        .activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    unsafe { set_front_process() };
}

/// Carbon SetFrontProcessWithOptions: 全局热键唤起的无 bundle 进程需要这条路径才能拿到前台
#[cfg(target_os = "macos")]
unsafe fn set_front_process() {
    #[repr(C)]
    struct ProcessSerialNumber {
        high_long_of_psn: u32,
        low_long_of_psn: u32,
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn GetCurrentProcess(psn: *mut ProcessSerialNumber) -> i32;
        fn SetFrontProcessWithOptions(psn: *const ProcessSerialNumber, options: u32) -> i32;
    }
    const K_SET_FRONT_PROCESS_FRONT_WINDOW_ONLY: u32 = 1;
    let mut psn = ProcessSerialNumber {
        high_long_of_psn: 0,
        low_long_of_psn: 0,
    };
    unsafe {
        GetCurrentProcess(&mut psn);
        SetFrontProcessWithOptions(&psn, K_SET_FRONT_PROCESS_FRONT_WINDOW_ONLY);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn activate_app() {}

/// 隐藏窗口左上角的红绿灯按钮
#[cfg(target_os = "macos")]
fn hide_traffic_lights(window: &mut Window) {
    use objc2_app_kit::{NSView, NSWindowButton};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let nsview: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
    let Some(nswindow) = nsview.window() else {
        return;
    };
    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = nswindow.standardWindowButton(kind) {
            button.setHidden(true);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn hide_traffic_lights(_: &mut Window) {}

/// 禁用标题栏双击放大:在本应用 defaults 域写 AppleActionOnDoubleClick = None,
/// 覆盖全局域设置,只对 ePostit 生效
#[cfg(target_os = "macos")]
fn disable_titlebar_double_click() {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;
    unsafe {
        let defaults: *mut AnyObject = msg_send![class!(NSUserDefaults), standardUserDefaults];
        let key = NSString::from_str("AppleActionOnDoubleClick");
        let value = NSString::from_str("None");
        let _: () = msg_send![&*defaults, setObject: &*value, forKey: &*key];
    }
}

#[cfg(not(target_os = "macos"))]
fn disable_titlebar_double_click() {}

/// 把窗口移动到鼠标所在显示屏的中央;找不到含鼠标的屏时不动
#[cfg(target_os = "macos")]
fn move_window_to_mouse_screen(window: &mut Window) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let nsview: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
    let Some(nswindow) = nsview.window() else {
        return;
    };
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let mouse = NSEvent::mouseLocation();
    for screen in NSScreen::screens(mtm).iter() {
        let f = screen.frame();
        let contains = mouse.x >= f.origin.x
            && mouse.x < f.origin.x + f.size.width
            && mouse.y >= f.origin.y
            && mouse.y < f.origin.y + f.size.height;
        if !contains {
            continue;
        }
        let vf = screen.visibleFrame();
        let wf = nswindow.frame();
        let origin = objc2_foundation::NSPoint::new(
            vf.origin.x + (vf.size.width - wf.size.width) / 2.0,
            vf.origin.y + (vf.size.height - wf.size.height) / 2.0,
        );
        nswindow.setFrameOrigin(origin);
        return;
    }
}

#[cfg(not(target_os = "macos"))]
fn move_window_to_mouse_screen(_: &mut Window) {}

/// 隐藏 Dock 图标,成为菜单栏常驻应用
#[cfg(target_os = "macos")]
fn set_accessory() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

#[cfg(not(target_os = "macos"))]
fn set_accessory() {}
