use gpui_kit::*;

pub const UI_FONT: &str = "JetBrains Mono";
pub const CJK_FONT: &str = "Source Han Serif CN";

const THEME_JSON: &str = include_str!("theme.json");
const THEME_NAME: &str = "ePostit";

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

/// prompts.petals.top 配色,全部颜色在 theme.json 中声明
pub fn apply(cx: &mut App) {
    component::ThemeRegistry::global_mut(cx)
        .load_themes_from_str(THEME_JSON)
        .expect("load epostit theme");
    let config = component::ThemeRegistry::global(cx).themes()[THEME_NAME].clone();
    component::Theme::update(cx, |theme| {
        theme.apply_config(&config);
    });
}
