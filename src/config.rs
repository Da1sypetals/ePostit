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
    /// 唤起窗口时是否把窗口居中到鼠标所在显示屏;
    /// 关闭后窗口在记忆的位置唤起,适合固定位置的工作流
    pub follow_mouse: bool,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub sort: SortMode,
    /// theme.json 中的主题名:morganite、forest、dawn、nocturne
    pub theme: String,
    /// 配置文件自身的路径,save 只写入该路径;
    /// 测试构造的实例可指向临时文件,避免写入真实配置
    #[serde(skip)]
    pub path: PathBuf,
}

impl Global for Config {}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "cmd-n".into(),
            notes_dir: default_notes_dir(),
            hide_on_blur: true,
            follow_mouse: true,
            sidebar_width: 240.,
            sidebar_collapsed: false,
            sort: SortMode::LastEdited,
            theme: "morganite".into(),
            path: config_path(),
        }
    }
}

pub fn app_dir() -> PathBuf {
    // EPOSTIT_DATA_DIR:e2e 测试把数据目录指到独立位置,
    // 配置、笔记、日志与正式实例完全隔离
    if let Some(dir) = std::env::var_os("EPOSTIT_DATA_DIR") {
        return PathBuf::from(dir);
    }
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
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut config: Self = toml::from_str(&text).expect("parse config.toml");
                config.path = path.to_path_buf();
                config
            }
            Err(_) => Self {
                path: path.to_path_buf(),
                ..Self::default()
            },
        }
    }

    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).expect("create config dir");
        }
        let text = toml::to_string_pretty(self).expect("serialize config");
        std::fs::write(&self.path, text).expect("write config.toml");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// save 必须写入实例自身的 path 并从同一路径读回;
    /// 指向固定全局路径的实现会让测试改写真实用户配置
    #[test]
    fn save_writes_own_path_and_loads_back() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-config")
            .join("roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.toml");

        let mut config = Config::load_from(&path);
        assert_eq!(config.hotkey, "cmd-n");
        config.hotkey = "cmd+alt+k".into();
        config.save();

        let loaded = Config::load_from(&path);
        assert_eq!(loaded.hotkey, "cmd+alt+k");
        assert_eq!(loaded.path, path);
        assert!(loaded.hide_on_blur);
    }
}
