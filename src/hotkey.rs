use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use gpui_kit::*;
use std::sync::{Mutex, OnceLock};

static MANAGER: OnceLock<GlobalHotKeyManager> = OnceLock::new();
static HOTKEY: Mutex<Option<HotKey>> = Mutex::new(None);
static REGISTERED: Mutex<bool> = Mutex::new(false);

/// 解析 "cmd-n" 形式的快捷键字符串
fn parse_hotkey(s: &str) -> HotKey {
    let mut mods = Modifiers::empty();
    let mut code = None;
    for part in s.split('-').map(|p| p.trim().to_lowercase()) {
        match part.as_str() {
            "cmd" | "super" | "meta" => mods |= Modifiers::SUPER,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            _ => code = Some(parse_code(&part)),
        }
    }
    HotKey::new(Some(mods), code.expect("hotkey key code"))
}

fn parse_code(key: &str) -> Code {
    match key {
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "space" => Code::Space,
        other => panic!("unsupported hotkey key: {other}"),
    }
}

pub fn install(cx: &mut App, hotkey_str: &str) {
    MANAGER
        .set(GlobalHotKeyManager::new().expect("hotkey manager"))
        .ok()
        .expect("hotkey manager init once");
    *HOTKEY.lock().unwrap() = Some(parse_hotkey(hotkey_str));

    // 事件桥:global-hotkey 的 crossbeam receiver -> gpui 主循环
    let (tx, rx) = async_channel::unbounded::<()>();
    std::thread::spawn(move || {
        while GlobalHotKeyEvent::receiver().recv().is_ok() {
            let _ = tx.send_blocking(());
        }
    });
    cx.spawn(async move |cx| {
        while rx.recv().await.is_ok() {
            cx.update(|cx| crate::toggle_window(cx));
        }
    })
    .detach();
}

pub fn register() {
    let mut registered = REGISTERED.lock().unwrap();
    if *registered {
        return;
    }
    let (Some(hotkey), Some(manager)) = (*HOTKEY.lock().unwrap(), MANAGER.get()) else {
        return;
    };
    manager.register(hotkey).expect("register hotkey");
    *registered = true;
}

pub fn unregister() {
    let mut registered = REGISTERED.lock().unwrap();
    if !*registered {
        return;
    }
    let (Some(hotkey), Some(manager)) = (*HOTKEY.lock().unwrap(), MANAGER.get()) else {
        return;
    };
    manager.unregister(hotkey).expect("unregister hotkey");
    *registered = false;
}
