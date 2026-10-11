use gpui_kit::Global;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    #[default]
    LastEdited,
    Created,
    Title,
}

impl SortMode {
    pub const LABELS: [&'static str; 3] = ["Last edited", "Created", "Title"];

    pub fn from_label(label: &str) -> Self {
        match label {
            "Created" => Self::Created,
            "Title" => Self::Title,
            _ => Self::LastEdited,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub hotkey: String,
    pub notes_dir: PathBuf,
    pub hide_on_blur: bool,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub sort: SortMode,
}

impl Global for Config {}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "cmd-n".into(),
            notes_dir: default_notes_dir(),
            hide_on_blur: true,
            sidebar_width: 240.,
            sidebar_collapsed: false,
            sort: SortMode::LastEdited,
        }
    }
}

pub fn app_dir() -> PathBuf {
    dirs::data_dir().expect("data dir").join("epostit")
}

pub fn default_notes_dir() -> PathBuf {
    app_dir().join("notes")
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.toml")
}

impl Config {
    pub fn load() -> Self {
        match std::fs::read_to_string(config_path()) {
            Ok(text) => toml::from_str(&text).expect("parse config.toml"),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        std::fs::create_dir_all(app_dir()).expect("create app dir");
        let text = toml::to_string_pretty(self).expect("serialize config");
        std::fs::write(config_path(), text).expect("write config.toml");
    }
}
