use crate::config::Config;
use gpui_kit::*;
use std::sync::Arc;

pub const UI_FONT: &str = "JetBrains Mono";
pub const CJK_FONT: &str = "Source Han Serif CN";

/// 每套主题一个文件
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

/// 全局字体:拉丁字符 JetBrains Mono,中文回退思源宋体;
/// features 在字体加载时写入 CTFont descriptor,连字全部关闭
pub fn app_font() -> Font {
    Font {
        family: UI_FONT.into(),
        features: no_ligatures(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            CJK_FONT.into(),
            "Source Han Serif SC".into(),
            "Noto Serif SC".into(),
            "Songti SC".into(),
        ])),
        ..font(UI_FONT)
    }
}

/// 关闭全部连字与上下文变形特性,文本按输入原样渲染
pub fn no_ligatures() -> FontFeatures {
    FontFeatures(Arc::new(vec![
        ("calt".into(), 0),
        ("liga".into(), 0),
        ("clig".into(), 0),
        ("dlig".into(), 0),
        ("hlig".into(), 0),
    ]))
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

#[cfg(test)]
mod tests {
    use super::no_ligatures;

    /// Font.features 经 FontKey 进入 load_family,加载出的 CTFont
    /// 直接带有特性关闭设置,这是 macOS 上唯一到达 shaping 的路径
    #[test]
    fn app_font_carries_disabled_ligatures() {
        let font = super::app_font();
        let tags = font.features.tag_value_list();
        for tag in ["calt", "liga", "clig", "dlig", "hlig"] {
            assert!(tags.contains(&(tag.to_string(), 0)), "{tag}");
        }
    }
}
