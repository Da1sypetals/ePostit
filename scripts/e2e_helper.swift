import AppKit
import CoreGraphics

// 全局热键端到端测试辅助,全部命令针对按 PID 指定的测试实例,
// 与同名的正式实例互不相干:
//   onscreen <pid>      主窗口在屏状态: "1 x y w h" 或 "0"
//   frontmost           前台应用 PID,用于断言测试实例从未取得焦点
//   writeconfig <dir>   写测试配置:热键 cmd+;,避免与正式实例的 cmd+n 冲突

let args = CommandLine.arguments

switch args[1] {
case "onscreen":
    let pid = Int32(args[2])!
    let list = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as! [[String: Any]]
    for w in list
    where (w["kCGWindowOwnerPID"] as? Int32) == pid
        && (w["kCGWindowName"] as? String) == "ePostit"
    {
        let on = (w["kCGWindowIsOnscreen"] as? Int) ?? 0
        if on == 1, let b = w["kCGWindowBounds"] as? [String: Any] {
            let v = { (k: String) in Int(b[k] as? Double ?? -99999) }
            print("1 \(v("X")) \(v("Y")) \(v("Width")) \(v("Height"))")
        } else {
            print(on)
        }
        exit(0)
    }
    print(0)

case "frontmost":
    print(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1)

case "writeconfig":
    // cmd+; 只在测试实例注册;按键落在其他应用时完全无副作用
    let text = """
    hotkey = "cmd+;"
    follow_mouse = false

    """
    try! text.write(
        toFile: args[2] + "/config.toml",
        atomically: true,
        encoding: .utf8
    )

default:
    exit(64)
}
