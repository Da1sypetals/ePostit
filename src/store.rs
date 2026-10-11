use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use ulid::Ulid;

#[derive(Debug, Serialize, Deserialize)]
struct FrontMatter {
    title: String,
    created: DateTime<Local>,
    #[serde(default)]
    archived: bool,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub created: DateTime<Local>,
    pub modified: DateTime<Local>,
    pub archived: bool,
    pub path: PathBuf,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        fs::create_dir_all(&dir).expect("create notes dir");
        Self { dir }
    }

    /// 扫描目录加载全部笔记的元数据
    pub fn scan(&self) -> Vec<Note> {
        let mut notes = Vec::new();
        let entries = fs::read_dir(&self.dir).expect("read notes dir");
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("note filename")
                .to_string();
            let meta = fs::metadata(&path).expect("note metadata");
            let modified = meta.modified().map(to_local).expect("note mtime");
            let text = fs::read_to_string(&path).expect("read note");
            let (title, created, archived) = match split_front_matter(&text) {
                Some((yaml, _)) => match serde_yml::from_str::<FrontMatter>(yaml) {
                    Ok(fm) => (fm.title, fm.created, fm.archived),
                    Err(_) => {
                        let (title, created) = fallback_meta(&id, &meta);
                        (title, created, false)
                    }
                },
                None => {
                    let (title, created) = fallback_meta(&id, &meta);
                    // 外部放入的文件补写 front matter
                    let body = text;
                    write_note(&path, &title, created, false, &body);
                    (title, created, false)
                }
            };
            notes.push(Note {
                id,
                title,
                created,
                modified,
                archived,
                path,
            });
        }
        notes
    }

    /// 新建一条 Untitled 笔记并写入文件
    pub fn create(&self) -> Note {
        let id = Ulid::generate().to_string();
        let path = self.dir.join(format!("{id}.md"));
        let title = "Untitled".to_string();
        let created = Local::now();
        write_note(&path, &title, created, false, "");
        Note {
            id,
            title,
            created,
            modified: created,
            archived: false,
            path,
        }
    }

    pub fn body(&self, note: &Note) -> String {
        let text = fs::read_to_string(&note.path).expect("read note body");
        match split_front_matter(&text) {
            Some((_, body)) => body.to_string(),
            None => text,
        }
    }

    /// 保存正文与标题,mtime 随之更新
    pub fn save(&self, note: &mut Note, body: &str) {
        write_note(&note.path, &note.title, note.created, note.archived, body);
        note.modified = Local::now();
    }

    /// 改名只改写 front matter,不动正文
    pub fn rename(&self, note: &mut Note, title: &str) {
        let body = self.body(note);
        note.title = title.to_string();
        write_note(&note.path, &note.title, note.created, note.archived, &body);
    }

    /// 切换归档状态,只改写 front matter
    pub fn set_archived(&self, note: &mut Note, archived: bool) {
        let body = self.body(note);
        note.archived = archived;
        write_note(&note.path, &note.title, note.created, archived, &body);
    }

    /// 删除笔记,移入 .trash 目录
    pub fn trash(&self, note: &Note) {
        let trash = self.dir.join(".trash");
        fs::create_dir_all(&trash).expect("create trash dir");
        let target = trash.join(format!("{}.md", note.id));
        fs::rename(&note.path, target).expect("trash note");
    }
}

fn to_local(t: SystemTime) -> DateTime<Local> {
    DateTime::<Local>::from(t)
}

fn fallback_meta(id: &str, meta: &fs::Metadata) -> (String, DateTime<Local>) {
    let created = meta
        .created()
        .map(to_local)
        .unwrap_or_else(|_| Local::now());
    (id.to_string(), created)
}

/// 把文件内容分成 front matter YAML 与正文;文件不以 --- 开头时返回 None
fn split_front_matter(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text.strip_prefix("---")?;
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))?;
    for (idx, _) in rest.match_indices("\n---") {
        let after = &rest[idx + 4..];
        let valid_close = after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n");
        if valid_close {
            let body = after
                .strip_prefix('\n')
                .or_else(|| after.strip_prefix("\r\n"))
                .unwrap_or(after);
            return Some((&rest[..idx], body));
        }
    }
    None
}

/// 先写临时文件再 rename,避免写一半留下损坏的笔记
fn write_note(path: &PathBuf, title: &str, created: DateTime<Local>, archived: bool, body: &str) {
    let fm = serde_yml::to_string(&FrontMatter {
        title: title.to_string(),
        created,
        archived,
    })
    .expect("serialize front matter");
    let text = format!("---\n{}\n---\n{body}", fm.trim_end());
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, text).expect("write note tmp");
    fs::rename(&tmp, path).expect("commit note write");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-notes")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn create_scan_roundtrip() {
        let dir = dir("create_scan_roundtrip");
        let store = Store::new(dir.clone());
        let note = store.create();
        assert!(note.path.exists());
        assert_eq!(note.title, "Untitled");

        let notes = store.scan();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, note.id);
        assert_eq!(notes[0].title, "Untitled");
        assert_eq!(store.body(&notes[0]), "");
    }

    #[test]
    fn duplicate_titles_have_distinct_files() {
        let dir = dir("duplicate_titles");
        let store = Store::new(dir.clone());
        let mut a = store.create();
        let mut b = store.create();
        store.rename(&mut a, "Same Name");
        store.rename(&mut b, "Same Name");
        assert_ne!(a.path, b.path);

        let titles: Vec<_> = store.scan().iter().map(|n| n.title.clone()).collect();
        assert_eq!(titles, vec!["Same Name", "Same Name"]);
    }

    #[test]
    fn rename_keeps_filename_and_body() {
        let dir = dir("rename");
        let store = Store::new(dir.clone());
        let mut note = store.create();
        store.save(&mut note, "body stays");
        let path = note.path.clone();
        store.rename(&mut note, "新标题");

        assert_eq!(note.path, path);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("title: 新标题"));
        assert!(text.ends_with("body stays"));
        assert_eq!(store.body(&note), "body stays");
    }

    #[test]
    fn trash_moves_file_out_of_scan() {
        let dir = dir("trash");
        let store = Store::new(dir.clone());
        let note = store.create();
        store.trash(&note);

        assert!(store.scan().is_empty());
        assert!(dir.join(".trash").join(format!("{}.md", note.id)).exists());
    }

    #[test]
    fn save_overwrites_body() {
        let dir = dir("save");
        let store = Store::new(dir.clone());
        let mut note = store.create();
        store.save(&mut note, "first");
        store.save(&mut note, "second");
        assert_eq!(store.body(&note), "second");
    }

    #[test]
    fn archive_roundtrip() {
        let dir = dir("archive_roundtrip");
        let store = Store::new(dir.clone());
        let mut note = store.create();
        store.save(&mut note, "keep me");
        store.set_archived(&mut note, true);
        assert!(note.archived);

        let mut notes = store.scan();
        assert!(notes[0].archived);
        assert_eq!(store.body(&notes[0]), "keep me");

        store.set_archived(&mut notes[0], false);
        assert!(!store.scan()[0].archived);
    }

    #[test]
    fn missing_archived_defaults_false() {
        let dir = dir("archived_default");
        fs::create_dir_all(&dir).unwrap();
        // 旧格式文件没有 archived 字段
        fs::write(
            dir.join("01J0000000000000000000AA.md"),
            "---\ntitle: Old\ncreated: 2024-01-01T00:00:00+00:00\n---\nbody",
        )
        .unwrap();
        let notes = Store::new(dir).scan();
        assert_eq!(notes.len(), 1);
        assert!(!notes[0].archived);
    }
}
