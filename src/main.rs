mod applog;
mod config;
mod hotkey;
mod keymap;
mod login_item;
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

/// 唤起计数,失焦延迟隐藏任务以它判定期间是否发生唤起
static SHOW_REQUESTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn show_count() -> u64 {
    SHOW_REQUESTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// 请求隐藏应用;headless 测试平台没有真实窗口可隐藏,只记录请求
/// 隐藏后窗口必然失焦,就地注册全局热键,不依赖激活观察或 render 的时序
pub fn request_hide(cx: &mut App) {
    applog::line("request_hide");
    hotkey::register();
    if !cfg!(test) {
        cx.hide();
    }
}

/// 显示并聚焦窗口,焦点进编辑器;
/// EPOSTIT_NO_ACTIVATE 下只把窗口排到最前,不激活应用
pub fn show_window(cx: &mut App) {
    SHOW_REQUESTED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let Some(handle) = cx.global::<Shared>().window else {
        return;
    };
    let _ = cx.update_window(handle, |_, window, cx| {
        if no_activate() {
            unhide_without_activation();
            order_window_front(window);
        } else {
            if cx.global::<Config>().follow_mouse {
                move_window_to_mouse_screen(window);
            }
            activate_app();
            window.activate_window();
        }
        if let Some(root) = shared_root(cx) {
            root.update(cx, |r, cx| r.focus_editor(window, cx));
        }
    });
}

/// 显示窗口并新建一条笔记
pub fn new_note(cx: &mut App) {
    SHOW_REQUESTED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let Some(handle) = cx.global::<Shared>().window else {
        return;
    };
    let _ = cx.update_window(handle, |_, window, cx| {
        if no_activate() {
            unhide_without_activation();
            order_window_front(window);
        } else {
            activate_app();
            window.activate_window();
        }
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

/// 开关式唤起:窗口可见且为 key 则隐藏,否则显示。
/// 判定直接查询 NSWindow 的 isVisible/isKeyWindow 同步真值,
/// 热键事件先于激活通知到达时不会误读滞后的激活标记
pub fn toggle_window(cx: &mut App) {
    let Some(handle) = cx.global::<Shared>().window else {
        return;
    };
    let (visible, key) = cx
        .update_window(handle, |_, window, _| window_visibility_state(window))
        .unwrap_or((false, false));
    // 不激活模式下窗口永远不会成为 key,以可见性为准
    if visible && (key || no_activate()) {
        hide_window(cx);
    } else {
        show_window(cx);
    }
}

/// NSWindow 同步状态:是否在屏、是否 key window
#[cfg(target_os = "macos")]
fn window_visibility_state(window: &Window) -> (bool, bool) {
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return (false, false);
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return (false, false);
    };
    let nsview: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
    match nsview.window() {
        Some(nswindow) => (nswindow.isVisible(), nswindow.isKeyWindow()),
        None => (false, false),
    }
}

#[cfg(not(target_os = "macos"))]
fn window_visibility_state(window: &Window) -> (bool, bool) {
    (window.is_window_active(), window.is_window_active())
}

fn main() {
    enforce_single_instance();
    applog::init();
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
            persist_window_frame(window);
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
        if !test_isolation() {
            tray::install(cx);
        }
        set_accessory();
        if !no_activate() && !start_hidden() {
            activate_app();
        }
    });
}

fn window_options(cx: &App) -> WindowOptions {
    // EPOSTIT_DISPLAY_ID:e2e 测试把窗口创建到指定显示器(不可见虚拟屏)
    let display_id = std::env::var("EPOSTIT_DISPLAY_ID")
        .ok()
        .map(|v| DisplayId::from(v.parse::<u64>().expect("EPOSTIT_DISPLAY_ID")));
    WindowOptions {
        display_id,
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            display_id,
            size(px(880.), px(560.)),
            cx,
        ))),
        focus: !no_activate() && !start_hidden(),
        show: !start_hidden(),
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

/// 单实例:存在同 bundle id 的其他进程时激活它并退出,
/// 避免两个实例同时注册全局热键后各自 toggle 互相抵消
#[cfg(target_os = "macos")]
fn enforce_single_instance() {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    if test_isolation() {
        return;
    }
    use objc2_foundation::NSBundle;
    let Some(bundle_id) = NSBundle::mainBundle().bundleIdentifier() else {
        return;
    };
    let me = NSRunningApplication::currentApplication().processIdentifier();
    for app in NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle_id).iter() {
        if app.processIdentifier() == me {
            continue;
        }
        app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
        std::process::exit(0);
    }
}

#[cfg(not(target_os = "macos"))]
fn enforce_single_instance() {}

/// 激活应用(菜单栏应用唤起窗口的强制激活路径);
/// 先同步 unhide:activateWithOptions 的解除隐藏异步生效,
/// 隐藏状态下执行的 makeKeyAndOrderFront 对窗口无效
#[cfg(target_os = "macos")]
pub fn activate_app() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    NSApplication::sharedApplication(mtm).unhide(None);
    NSRunningApplication::currentApplication().activateWithOptions(
        NSApplicationActivationOptions::ActivateAllWindows
            | NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    );
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
    // kSetFrontProcessCausedByUser: 以用户发起的语义把进程带到前台,
    // 否则系统会把焦点还给之前的前台应用,唤起后瞬间失焦又被自动隐藏
    const K_SET_FRONT_PROCESS_CAUSED_BY_USER: u32 = 2;
    let mut psn = ProcessSerialNumber {
        high_long_of_psn: 0,
        low_long_of_psn: 0,
    };
    unsafe {
        GetCurrentProcess(&mut psn);
        SetFrontProcessWithOptions(&psn, K_SET_FRONT_PROCESS_CAUSED_BY_USER);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn activate_app() {}

/// EPOSTIT_DATA_DIR:e2e 测试以独立数据目录运行第二实例,
/// 配置、笔记、日志与正式实例完全隔离
fn test_isolation() -> bool {
    std::env::var_os("EPOSTIT_DATA_DIR").is_some()
}

/// EPOSTIT_NO_ACTIVATE:唤起只把窗口排序到前,不激活应用,
/// e2e 测试期间不抢占用户前台焦点
fn no_activate() -> bool {
    std::env::var_os("EPOSTIT_NO_ACTIVATE").is_some()
}

/// EPOSTIT_START_HIDDEN:启动时窗口不显示、不激活,
/// 安装脚本用它拉起新实例,重启过程全程零画面
fn start_hidden() -> bool {
    std::env::var_os("EPOSTIT_START_HIDDEN").is_some()
}

/// 不激活应用地把窗口排到最前
#[cfg(target_os = "macos")]
fn order_window_front(window: &mut Window) {
    use objc2::msg_send;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let nsview: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
    if let Some(nswindow) = nsview.window() {
        unsafe {
            let _: () = msg_send![&*nswindow, orderFrontRegardless];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn order_window_front(_: &mut Window) {}

/// NSApplication.unhideWithoutActivation:解除隐藏状态但不取得前台
#[cfg(target_os = "macos")]
fn unhide_without_activation() {
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::NSApplication;
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    unsafe {
        let _: () = msg_send![&app, unhideWithoutActivation];
    }
}

#[cfg(not(target_os = "macos"))]
fn unhide_without_activation() {}

/// NSWindow frame autosave:窗口位置跨启动保留,
/// setFrameAutosaveName 会就地应用已保存的位置;
/// 隔离模式跳过,autosave 域按 bundle id 共享,测试实例不能写入
#[cfg(target_os = "macos")]
fn persist_window_frame(window: &mut Window) {
    use objc2_app_kit::NSView;
    use objc2_foundation::NSString;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if test_isolation() {
        return;
    }
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let nsview: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
    if let Some(nswindow) = nsview.window() {
        nswindow.setFrameAutosaveName(&NSString::from_str("ePostitMain"));
    }
}

#[cfg(not(target_os = "macos"))]
fn persist_window_frame(_: &mut Window) {}

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
