use crate::config::Config;
use gpui_kit::*;

pub const UI_FONT: &str = "JetBrains Mono";
pub const CJK_FONT: &str = "Source Han Serif CN";

/// 每套主题一个文件,由 scripts/split_themes.py 从 theme.json 生成
const THEME_FILES: [&str; 4] = [
    include_str!("themes/morganite.json"),
    include_str!("themes/forest.json"),
    include_str!("themes/dawn.json"),
    include_str!("themes/nocturne.json"),
];
const FALLBACK_THEME: &str = "morganite";

/// 主题 id 与设置界面显示名;顺序即下拉列表顺序
pub const THEME_CHOICES: [(&str, &str); 4] = [
    ("morganite", "Morganite"),
    ("forest", "Forest"),
    ("dawn", "Dawn"),
    ("nocturne", "Nocturne"),
];

/// 全局字体:拉丁字符 JetBrains Mono,中文回退思源宋体
pub fn app_font() -> Font {
    Font {
        family: UI_FONT.into(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            CJK_FONT.into(),
            "Source Han Serif SC".into(),
            "Noto Serif SC".into(),
            "Songti SC".into(),
        ])),
        ..font(UI_FONT)
    }
}

/// 加载主题集合并应用配置里选中的主题,未知名字回退到 morganite
pub fn apply(cx: &mut App) {
    let registry = component::ThemeRegistry::global_mut(cx);
    for file in THEME_FILES {
        registry
            .load_themes_from_str(file)
            .expect("load epostit themes");
    }
    set_theme(cx, &cx.global::<Config>().theme.clone());
}

/// 运行时切换主题,所有窗口的 token 取色即时更新
pub fn set_theme(cx: &mut App, name: &str) {
    let name = THEME_CHOICES
        .iter()
        .any(|(id, _)| *id == name)
        .then_some(name)
        .unwrap_or(FALLBACK_THEME);
    let config = component::ThemeRegistry::global(cx).themes()[name].clone();
    component::Theme::update(cx, |theme| {
        theme.apply_config(&config);
    });
}
