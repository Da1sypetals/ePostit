use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui_kit::*;
use std::sync::{Mutex, OnceLock};

static MANAGER: OnceLock<GlobalHotKeyManager> = OnceLock::new();
static HOTKEY: Mutex<Option<HotKey>> = Mutex::new(None);
static REGISTERED: Mutex<bool> = Mutex::new(false);

/// 修饰键前缀表;解析时按前缀逐个剥离,余下整段作为按键名,
/// 因此 "-" 这类本身可作分隔符的键也能表达(cmd+-、cmd--)
const MOD_PREFIXES: [(&[&str], Modifiers); 4] = [
    (&["cmd", "super", "meta"], Modifiers::SUPER),
    (&["ctrl", "control"], Modifiers::CONTROL),
    (&["alt", "option"], Modifiers::ALT),
    (&["shift"], Modifiers::SHIFT),
];

/// 解析 "cmd-n" / "cmd+alt+n" 形式的快捷键字符串,+ 与 - 分隔符均可
fn parse_hotkey(s: &str) -> HotKey {
    let mut mods = Modifiers::empty();
    let mut rest = s.trim().to_lowercase();
    loop {
        let mut next = None;
        for (names, m) in MOD_PREFIXES {
            for name in names {
                let Some(r) = rest
                    .strip_prefix(name)
                    .and_then(|r| r.strip_prefix(['+', '-']))
                else {
                    continue;
                };
                mods |= m;
                next = Some(r.to_string());
                break;
            }
            if next.is_some() {
                break;
            }
        }
        match next {
            Some(r) => rest = r,
            None => break,
        }
    }
    assert!(!rest.is_empty(), "hotkey missing key: {s}");
    HotKey::new(Some(mods), parse_code(&rest))
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
        "-" => Code::Minus,
        "=" => Code::Equal,
        "[" => Code::BracketLeft,
        "]" => Code::BracketRight,
        ";" => Code::Semicolon,
        "'" => Code::Quote,
        "," => Code::Comma,
        "." => Code::Period,
        "/" => Code::Slash,
        other => panic!("unsupported hotkey key: {other}"),
    }
}

/// Carbon 对按下与松开各派发一次事件,仅按下触发一次 toggle;
/// 若松开也 toggle,窗口刚激活时松开事件会再次进入隐藏分支,
/// 表现为唤起后瞬间又被隐藏
fn is_toggle_event(ev: &GlobalHotKeyEvent) -> bool {
    ev.state == HotKeyState::Pressed
}

pub fn install(cx: &mut App, hotkey_str: &str) {
    MANAGER
        .set(GlobalHotKeyManager::new().expect("hotkey manager"))
        .ok()
        .expect("hotkey manager init once");
    let parsed = parse_hotkey(hotkey_str);
    *HOTKEY.lock().unwrap() = Some(parsed);
    crate::applog::line(format_args!(
        "hotkey install: {hotkey_str} id={}",
        parsed.id()
    ));

    // 事件桥:global-hotkey 的 crossbeam receiver -> gpui 主循环
    let (tx, rx) = async_channel::unbounded::<()>();
    std::thread::spawn(move || {
        while let Ok(ev) = GlobalHotKeyEvent::receiver().recv() {
            if is_toggle_event(&ev) {
                crate::applog::line(format_args!("hotkey event id={}", ev.id));
                let _ = tx.send_blocking(());
            }
        }
        crate::applog::line("hotkey event receiver closed");
    });
    cx.spawn(async move |cx| {
        while rx.recv().await.is_ok() {
            // toggle 内的 panic 不能杀死事件循环,否则热键永久失效而进程仍存活;
            // panic 本身仍经由默认 hook 输出到日志
            let _ = cx.update(|cx| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::toggle_window(cx);
                }));
            });
        }
    })
    .detach();

    // 激活观察器只响应状态变化:后台启动(open -g、登录项 RunAtLoad)
    // 且从未被激活过的进程收不到任何激活事件,注册会永远不发生,
    // 按安装时的实际激活状态完成首次注册
    if cx.active_window().is_none() {
        register();
    }
}

pub fn register() {
    let mut registered = REGISTERED.lock().unwrap();
    if *registered {
        return;
    }
    let (Some(hotkey), Some(manager)) = (*HOTKEY.lock().unwrap(), MANAGER.get()) else {
        crate::applog::line("register skipped: no hotkey or manager");
        return;
    };
    manager.register(hotkey).expect("register hotkey");
    *registered = true;
    crate::applog::line(format_args!("registered id={}", hotkey.id()));
}

pub fn unregister() {
    let mut registered = REGISTERED.lock().unwrap();
    if !*registered {
        return;
    }
    let (Some(hotkey), Some(manager)) = (*HOTKEY.lock().unwrap(), MANAGER.get()) else {
        crate::applog::line("unregister skipped: no hotkey or manager");
        return;
    };
    manager.unregister(hotkey).expect("unregister hotkey");
    *registered = false;
    crate::applog::line(format_args!("unregistered id={}", hotkey.id()));
}

/// 换绑:替换存储的按键;当前已注册时注销旧键并注册新键,
/// 未注册时只更新存储,下一次 register 生效
pub fn rebind(new_str: &str) {
    let new = parse_hotkey(new_str);
    let old = HOTKEY.lock().unwrap().replace(new);
    let registered = REGISTERED.lock().unwrap();
    if !*registered {
        return;
    }
    let Some(manager) = MANAGER.get() else {
        return;
    };
    if let Some(old) = old {
        manager.unregister(old).expect("unregister hotkey");
    }
    manager.register(new).expect("register hotkey");
}

#[cfg(test)]
fn stored_hotkey() -> Option<HotKey> {
    *HOTKEY.lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::{is_toggle_event, parse_hotkey, rebind, stored_hotkey};
    #[cfg(target_os = "macos")]
    use global_hotkey::GlobalHotKeyManager;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, HotKeyState};

    /// 按下触发 toggle,松开必须被丢弃;
    /// Carbon 每按一次派发两个事件,松开再 toggle 会立即隐藏刚唤起的窗口
    #[test]
    fn only_pressed_events_toggle() {
        let pressed = GlobalHotKeyEvent {
            id: 1,
            state: HotKeyState::Pressed,
        };
        let released = GlobalHotKeyEvent {
            id: 1,
            state: HotKeyState::Released,
        };
        assert!(is_toggle_event(&pressed));
        assert!(!is_toggle_event(&released));
    }

    /// 设置面板写出的每个串必须解析回用户选中的键;
    /// id 是 Carbon EventHotKeyID 的注册键,比对 id 即比对注册本体
    #[test]
    fn parse_maps_to_selected_combo() {
        let cases = [
            ("cmd+n", HotKey::new(Some(Modifiers::SUPER), Code::KeyN)),
            ("cmd-n", HotKey::new(Some(Modifiers::SUPER), Code::KeyN)),
            (
                "cmd+alt+k",
                HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::KeyK),
            ),
            (
                "ctrl+option+9",
                HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Digit9),
            ),
            ("cmd+-", HotKey::new(Some(Modifiers::SUPER), Code::Minus)),
            ("cmd+=", HotKey::new(Some(Modifiers::SUPER), Code::Equal)),
        ];
        for (s, expected) in cases {
            assert_eq!(parse_hotkey(s), expected, "{s}");
        }
    }

    /// 换绑后存储的按键必须跟随新串;注册路径只读 HOTKEY,
    /// 存储值与写入的配置不一致时全局热键即按错键注册
    #[test]
    fn rebind_updates_stored_hotkey() {
        rebind("cmd+alt+k");
        assert_eq!(stored_hotkey(), Some(parse_hotkey("cmd+alt+k")));
        rebind("ctrl+/");
        assert_eq!(stored_hotkey(), Some(parse_hotkey("ctrl+/")));
    }

    /// 真实的 OS 级注册验证,直接走 Carbon RegisterEventHotKey:
    /// 设置界面可产生的每个尾键都必须注册成功,高频注册注销循环不产生错误。
    /// 选用生僻组合做循环避免与运行中的实例争抢真实按键;
    /// 单一 manager 与串行断言对应单一应用实例的真实形态
    #[cfg(target_os = "macos")]
    #[test]
    fn bindable_keys_register_with_os_repeatedly() {
        let manager = GlobalHotKeyManager::new().expect("hotkey manager");
        let cycle = parse_hotkey("ctrl+alt+shift+/");
        for _ in 0..30 {
            manager.register(cycle).expect("register cycle");
            manager.unregister(cycle).expect("unregister cycle");
        }
        for key in "abcdefghijklmnopqrstuvwxyz0123456789-=[]';,./".chars() {
            let combo = format!("cmd+{key}");
            let hotkey = parse_hotkey(&combo);
            manager
                .register(hotkey)
                .unwrap_or_else(|e| panic!("register {combo}: {e}"));
            manager
                .unregister(hotkey)
                .unwrap_or_else(|e| panic!("unregister {combo}: {e}"));
        }
    }
}
