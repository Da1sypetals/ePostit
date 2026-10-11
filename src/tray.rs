use gpui_kit::*;
use muda::{Icon as MenuIcon, IconMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const ID_NEW: &str = "new_note";
const ID_SHOW: &str = "show";
const ID_OPEN: &str = "open_storage";

const FINDER_RGBA: &[u8] = include_bytes!("../assets/icons/finder.rgba");

thread_local! {
    static TRAY: std::cell::RefCell<Option<TrayIcon>> = const { std::cell::RefCell::new(None) };
}

/// 安装菜单栏图标与菜单,事件经 channel 送进 gpui 主循环
pub fn install(cx: &mut App) {
    let menu = Menu::new();
    let item_new = MenuItem::with_id(ID_NEW, "New Note", true, None);
    let item_show = MenuItem::with_id(ID_SHOW, "Show ePostit", true, None);
    let item_open = IconMenuItem::with_id(
        ID_OPEN,
        "Open Storage Folder",
        true,
        Some(MenuIcon::from_rgba(FINDER_RGBA.to_vec(), 32, 32).expect("finder menu icon")),
        None,
    );
    let item_quit = PredefinedMenuItem::quit(Some("Quit ePostit"));
    menu.append_items(&[
        &item_new,
        &item_show,
        &item_open,
        &PredefinedMenuItem::separator(),
        &item_quit,
    ])
    .expect("build tray menu");

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("ePostit")
        .with_icon_templated(make_icon())
        .build()
        .expect("build tray icon");
    TRAY.with(|t| *t.borrow_mut() = Some(tray));

    let (tx, rx) = async_channel::unbounded::<TrayAction>();
    // 菜单事件
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            while let Ok(event) = MenuEvent::receiver().recv() {
                let action = match event.id.0.as_str() {
                    ID_NEW => TrayAction::NewNote,
                    ID_SHOW => TrayAction::Show,
                    ID_OPEN => TrayAction::OpenStorage,
                    _ => continue,
                };
                let _ = tx.send_blocking(action);
            }
        });
    }
    // 图标点击:左键切换窗口
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            while let Ok(event) = TrayIconEvent::receiver().recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Down,
                    ..
                } = event
                {
                    let _ = tx.send_blocking(TrayAction::Toggle);
                }
            }
        });
    }

    cx.spawn(async move |cx| {
        while let Ok(action) = rx.recv().await {
            cx.update(|cx| match action {
                TrayAction::Toggle => crate::toggle_window(cx),
                TrayAction::Show => crate::show_window(cx),
                TrayAction::NewNote => crate::new_note(cx),
                TrayAction::OpenStorage => crate::open_storage(cx),
            });
        }
    })
    .detach();
}

#[derive(Clone, Copy)]
enum TrayAction {
    Toggle,
    Show,
    NewNote,
    OpenStorage,
}

/// 菜单栏模板图标,由 assets/icon-src.png 经 scripts/make_icons.py 生成
fn make_icon() -> Icon {
    const N: u32 = 44;
    let rgba = include_bytes!("../assets/tray-icon.rgba").to_vec();
    Icon::from_rgba(rgba, N, N).expect("tray icon")
}
