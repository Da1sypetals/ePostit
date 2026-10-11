use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "top.petals.epostit";

fn plist_path() -> PathBuf {
    dirs::home_dir()
        .expect("home dir")
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

/// 登录项指向当前可执行文件;.app 内运行时即 Contents/MacOS/epostit
fn program_path() -> PathBuf {
    std::env::current_exe().expect("current exe")
}

/// LaunchAgent 文件存在即视为已启用
pub fn is_enabled() -> bool {
    plist_path().exists()
}

pub fn set_enabled(enabled: bool) {
    let plist = plist_path();
    // 先卸载旧记录;服务不存在时 launchctl 返回非零属正常情况
    let _ = Command::new("launchctl")
        .args(["unload", "-w"])
        .arg(&plist)
        .status();
    if !enabled {
        let _ = std::fs::remove_file(&plist);
        return;
    }
    let text = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
        program_path().display()
    );
    std::fs::create_dir_all(plist.parent().expect("plist parent"))
        .expect("create LaunchAgents dir");
    std::fs::write(&plist, text).expect("write launch agent plist");
    let status = Command::new("launchctl")
        .args(["load", "-w"])
        .arg(&plist)
        .status()
        .expect("launchctl load");
    assert!(status.success(), "launchctl load failed");
}
