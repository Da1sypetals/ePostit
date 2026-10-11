use std::io::Write;
use std::sync::Mutex;

static LOG: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// 初始化文件日志,写入 app_dir()/epostit.log;
/// 超过 256KB 时截断重开,测试进程不调用 init,line 直接跳过
pub fn init() {
    let path = crate::config::app_dir().join("epostit.log");
    let oversize = std::fs::metadata(&path)
        .map(|m| m.len() > 256 * 1024)
        .unwrap_or(false);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(oversize)
        .append(!oversize)
        .open(path)
        .expect("open log file");
    *LOG.lock().unwrap() = Some(file);
}

pub fn line(msg: impl std::fmt::Display) {
    let mut guard = LOG.lock().unwrap();
    let Some(file) = guard.as_mut() else {
        return;
    };
    let ts = chrono::Local::now().format("%H:%M:%S%.3f");
    let _ = writeln!(file, "{ts} {msg}");
    let _ = file.flush();
}
