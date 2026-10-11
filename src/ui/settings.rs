use crate::config::Config;
use crate::keymap::SETTINGS_CONTEXT;
use gpui_kit::base::IndexPath;
use gpui_kit::base::StyledExt;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

const MOD_LABELS: [&str; 3] = ["Cmd", "Option", "Ctrl"];
const MOD_NAMES: [&str; 3] = ["cmd", "alt", "ctrl"];
const MOD2_LABELS: [&str; 4] = ["None", "Cmd", "Option", "Ctrl"];

/// 快捷键尾键允许字符:字母、数字、-=[]';,./
fn valid_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-=[]';,./".contains(c)
}

/// 把已存快捷键字符串拆成修饰键规范名列表和尾键;
/// 前缀逐个剥离,+ 与 - 分隔符均可,尾键允许 "-" 这类分隔符字符
fn split_combo(s: &str) -> (Vec<&'static str>, String) {
    let mut mods = Vec::new();
    let mut rest = s.trim().to_lowercase();
    loop {
        let mut next = None;
        for (names, canon) in [
            (&["cmd", "super", "meta"][..], "cmd"),
            (&["ctrl", "control"][..], "ctrl"),
            (&["alt", "option"][..], "alt"),
            (&["shift"][..], "shift"),
        ] {
            for name in names {
                let Some(r) = rest
                    .strip_prefix(name)
                    .and_then(|r| r.strip_prefix(['+', '-']))
                else {
                    continue;
                };
                mods.push(canon);
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
    (mods, rest)
}

fn mod_index(name: Option<&str>) -> usize {
    name.and_then(|n| MOD_NAMES.iter().position(|m| *m == n))
        .unwrap_or(0)
}

fn label_to_name(label: &str) -> &'static str {
    MOD_LABELS
        .iter()
        .zip(MOD_NAMES)
        .find(|(l, _)| **l == label)
        .map(|(_, n)| n)
        .unwrap_or("cmd")
}

/// 由界面选项组成 "cmd+alt+n" 形式的存储串;第二个修饰键与第一个相同时去重,
/// 尾键非法或为空时返回 None
fn compose_hotkey(mod1: &str, mod2: Option<&str>, key: &str) -> Option<String> {
    let key = key.to_lowercase();
    let key = key.chars().next().filter(|c| valid_key_char(*c))?;
    let mut parts = vec![mod1];
    if let Some(m2) = mod2.filter(|m2| *m2 != mod1) {
        parts.push(m2);
    }
    Some(format!("{}+{}", parts.join("+"), key))
}

pub struct SettingsView {
    launch_at_login: bool,
    mod1: Entity<SelectState<Vec<SharedString>>>,
    mod2: Entity<SelectState<Vec<SharedString>>>,
    key_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (mods, key) = split_combo(&cx.global::<Config>().hotkey);
        let mod1 = cx.new(|cx| {
            SelectState::new(
                MOD_LABELS.map(SharedString::from).to_vec(),
                Some(IndexPath::default().row(mod_index(mods.first().copied()))),
                window,
                cx,
            )
        });
        let mod2 = cx.new(|cx| {
            SelectState::new(
                MOD2_LABELS.map(SharedString::from).to_vec(),
                Some(IndexPath::default().row(mods.get(1).map_or(0, |m| mod_index(Some(m)) + 1))),
                window,
                cx,
            )
        });
        let key_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(key)
                .placeholder("Key")
        });
        let subscriptions = vec![
            cx.subscribe_in(&mod1, window, |this, _, ev, _, cx| {
                if matches!(ev, SelectEvent::Confirm(_)) {
                    this.apply_hotkey(cx);
                }
            }),
            cx.subscribe_in(&mod2, window, |this, _, ev, _, cx| {
                if matches!(ev, SelectEvent::Confirm(_)) {
                    this.apply_hotkey(cx);
                }
            }),
            cx.subscribe_in(&key_input, window, |this, _, ev, window, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.on_key_change(window, cx);
                }
            }),
        ];
        Self {
            launch_at_login: crate::login_item::is_enabled(),
            mod1,
            mod2,
            key_input,
            _subscriptions: subscriptions,
        }
    }

    /// 尾键只保留最后一个合法字符,输入不合法时维持原内容
    fn on_key_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.key_input.read(cx).value().to_string().to_lowercase();
        let desired = raw
            .chars()
            .rev()
            .find(|c| valid_key_char(*c))
            .map(|c| c.to_string())
            .unwrap_or_default();
        if raw != desired {
            self.key_input
                .update(cx, |s, cx| s.set_value(desired, window, cx));
        }
        self.apply_hotkey(cx);
    }

    fn selected_name(&self, select: &Entity<SelectState<Vec<SharedString>>>, cx: &App) -> String {
        select
            .read(cx)
            .selected_value()
            .map(|v| v.to_string())
            .unwrap_or_default()
    }

    fn apply_hotkey(&mut self, cx: &mut Context<Self>) {
        let key = self.key_input.read(cx).value().to_string();
        let m1 = label_to_name(&self.selected_name(&self.mod1, cx));
        let m2_label = self.selected_name(&self.mod2, cx);
        let m2 = (m2_label != "None" && !m2_label.is_empty()).then(|| label_to_name(&m2_label));
        let Some(combo) = compose_hotkey(m1, m2, &key) else {
            return;
        };
        cx.global_mut::<Config>().hotkey = combo.clone();
        cx.global::<Config>().save();
        crate::hotkey::rebind(&combo);
    }

    fn toggle_login(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.launch_at_login = enabled;
        crate::login_item::set_enabled(enabled);
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let view = cx.entity().downgrade();
        v_flex()
            .key_context(SETTINGS_CONTEXT)
            .size_full()
            .bg(t.background)
            .text_color(t.foreground)
            .font(crate::theme::app_font())
            .p_5()
            .gap_5()
            .child(div().text_lg().font_semibold().child("Settings"))
            .child(
                Checkbox::new("launch-at-login")
                    .label("Launch at login")
                    .checked(self.launch_at_login)
                    .on_click(move |checked, _, cx| {
                        view.update(cx, |this, cx| this.toggle_login(*checked, cx))
                            .ok();
                    }),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().child("Global hotkey"))
                    // Select/Input 根容器会占满剩余宽度,定宽容器收住它们
                    .child(
                        div()
                            .w(px(88.))
                            .flex_shrink_0()
                            .child(Select::new(&self.mod1).small()),
                    )
                    .child(
                        div()
                            .w(px(88.))
                            .flex_shrink_0()
                            .child(Select::new(&self.mod2).small()),
                    )
                    .child(
                        div()
                            .w(px(56.))
                            .flex_shrink_0()
                            .child(Input::new(&self.key_input).small()),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingsView, compose_hotkey, split_combo, valid_key_char};
    use gpui_kit::base::IndexPath;

    #[test]
    fn split_combo_parses_modifiers_and_key() {
        assert_eq!(split_combo("cmd-n"), (vec!["cmd"], "n".to_string()));
        assert_eq!(
            split_combo("cmd+alt+k"),
            (vec!["cmd", "alt"], "k".to_string())
        );
        assert_eq!(
            split_combo("ctrl-option-3"),
            (vec!["ctrl", "alt"], "3".to_string())
        );
        assert_eq!(split_combo("cmd+-"), (vec!["cmd"], "-".to_string()));
        assert_eq!(split_combo("cmd--"), (vec!["cmd"], "-".to_string()));
        assert_eq!(
            split_combo("super+shift+/"),
            (vec!["cmd", "shift"], "/".to_string())
        );
    }

    #[test]
    fn compose_hotkey_dedupes_and_validates() {
        assert_eq!(compose_hotkey("cmd", None, "n"), Some("cmd+n".to_string()));
        assert_eq!(
            compose_hotkey("cmd", Some("alt"), "K"),
            Some("cmd+alt+k".to_string())
        );
        assert_eq!(
            compose_hotkey("cmd", Some("cmd"), "n"),
            Some("cmd+n".to_string())
        );
        assert_eq!(
            compose_hotkey("ctrl", Some("alt"), "-"),
            Some("ctrl+alt+-".to_string())
        );
        assert_eq!(compose_hotkey("cmd", None, "!"), None);
        assert_eq!(compose_hotkey("cmd", None, ""), None);
    }

    #[test]
    fn key_char_set() {
        for c in "az09-=[]';,./".chars() {
            assert!(valid_key_char(c), "{c}");
        }
        for c in "!@#$ ^&*()中".chars() {
            assert!(!valid_key_char(c), "{c}");
        }
    }

    use crate::config::Config;
    use gpui_kit::{
        AppContext, Entity, Focusable, TestAppContext, VisualTestContext, WindowOptions,
    };

    fn config_dir(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-config")
            .join(name)
    }

    fn harness<'a>(
        cx: &'a mut TestAppContext,
        name: &str,
        hotkey: &str,
    ) -> (Entity<SettingsView>, &'a mut VisualTestContext) {
        let dir = config_dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(cx);
            crate::keymap::bind(cx);
            cx.set_global(Config {
                hotkey: hotkey.to_string(),
                path: dir.join("config.toml"),
                ..Default::default()
            });
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SettingsView::new(window, cx))
            })
            .expect("open settings window")
        });
        let cx = VisualTestContext::from_window(handle, cx).into_mut();
        cx.cx.run_until_parked();
        (view, cx)
    }

    #[test]
    fn settings_reflects_current_hotkey() {
        let cx = &mut TestAppContext::single();
        let (view, cx) = harness(cx, "reflect", "cmd+alt+k");
        cx.update(|_, cx| {
            let this = view.read(cx);
            assert_eq!(this.key_input.read(cx).value().to_string(), "k");
            assert_eq!(this.selected_name(&this.mod1, cx), "Cmd");
            assert_eq!(this.selected_name(&this.mod2, cx), "Option");
        });
    }

    #[test]
    fn key_input_filters_and_rebinds() {
        let cx = &mut TestAppContext::single();
        let (view, cx) = harness(cx, "rebind", "cmd-n");
        cx.update(|window, cx| {
            let focus = view.read(cx).key_input.read(cx).focus_handle(cx).clone();
            focus.focus(window, cx);
        });
        // 非法字符不被填入,维持原值
        cx.simulate_input("!");
        cx.cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).key_input.read(cx).value().to_string(), "n");
        });
        // 合法字符替换尾键并立即换绑
        cx.simulate_input("k");
        cx.cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).key_input.read(cx).value().to_string(), "k");
            assert_eq!(cx.global::<Config>().hotkey, "cmd+k");
        });
        // 选择第二个修饰键组成三键快捷键
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.mod2.update(cx, |s, cx| {
                    s.set_selected_index(Some(IndexPath::default().row(2)), window, cx);
                });
                this.apply_hotkey(cx);
            });
        });
        cx.update(|_, cx| {
            assert_eq!(cx.global::<Config>().hotkey, "cmd+alt+k");
        });
        // 新键必须持久化到该实例自己的配置文件,重新读取能拿到同样的值
        let saved = Config::load_from(&config_dir("rebind").join("config.toml"));
        assert_eq!(saved.hotkey, "cmd+alt+k");
    }
}
