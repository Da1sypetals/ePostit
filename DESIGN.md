# ePostit 设计

一个常驻菜单栏的速记应用,Rust + GPUI 实现,仅面向 macOS。

## 依赖

| 用途 | 依赖 |
|---|---|
| UI 框架与全部控件 | `gpui-kit`(Longbridge 的 GPUI 组件库,内部固定匹配的 `gpui-pre-*` 版本,应用层只写 `use gpui_kit::*`,不单独声明 gpui) |
| 编辑器 | `gpui_kit::component::input::{Editor, EditorState}` |
| 侧栏控件 | gpui-kit 自带 `Select`、`Button`、`Scrollable` |
| 全局快捷键 | `global-hotkey`(底层 Carbon `RegisterEventHotKey`,不需要辅助功能权限) |
| 菜单栏图标 | `tray-icon` + `muda`(菜单) |
| 隐藏 Dock 图标、调整窗口属性 | `objc2-app-kit`(直接调用 NSApplication/NSWindow) |
| front matter 解析 | `gray_matter` 或 `serde_yml`,不手写 YAML 解析 |
| 笔记 id | `ulid` |
| 配置 | `serde` + `toml` |

## 编辑器

`EditorState` 是 ropey 文本引擎,自带能力:

- 撤销 Cmd+Z、重做 Cmd+Shift+Z、剪切/复制/粘贴/全选
- Cmd+F 搜索与替换
- 多光标:Cmd+Option+↑↓ 增删光标、Option+点击、Option+Shift+拖拽列选择
- Cmd+Backspace 删到行首、Cmd+Delete 删到行尾、Option+Backspace 删词
- 括号自动闭合、智能缩进、Tree-sitter 语法高亮(开 `tree-sitter-markdown` 特性)、行号、折叠

应用层在 `"Input"` 上下文里补充按键绑定(在 `gpui_kit::init` 之后调用 `cx.bind_keys`,后注册的绑定生效):

- 无选区时 Cmd+X 剪切整行:在 handler 里用 `selected_range()` / `set_selected_range()` 把选区扩到整行,再 `window.dispatch_action` 派发内置 `Cut`
- Cmd+Shift+K 删除整行、Cmd+Shift+D 复制整行、Option+↑↓ 移动整行
- Escape → `HideApp`(见下)

所有按键绑定集中在 `keymap` 模块,便于增改。

## 窗口与唤起

- 启动后调用 `NSApplication.setActivationPolicy(.accessory)` 隐藏 Dock 图标(gpui 启动时会强制设为 Regular,在 `run` 回调里覆盖)。应用常驻菜单栏:左键点击托盘图标切换窗口显隐,右键菜单为 New Note / Show / Quit。
- 全局 Cmd+N 是开关式唤起(Raycast 风格):
  - 窗口隐藏 → `cx.activate(true)`、聚焦窗口、焦点进编辑器,打开最近编辑的笔记
  - 窗口可见但未聚焦 → 聚焦
  - 窗口已聚焦 → `cx.hide()` 隐藏整个应用,焦点回到之前的应用
- 窗口聚焦期间注销全局热键,失焦时重新注册。这样应用内部按 Cmd+N 就是"新建笔记",与右上角铅笔按钮等价。
- Escape:只要应用处于聚焦状态,按一下立即 `cx.hide()` 关闭窗口。覆盖编辑器自身的 Escape 分层(多光标、搜索面板、右键菜单不拦截),改名 Input 里按 Escape 同样直接关闭窗口。
- 窗口关闭按钮走隐藏逻辑,进程常驻。
- 可配置项:`hide_on_blur`(失焦自动隐藏)与窗口 floating 置顶。

全局热键的固有行为是接管其他应用的同名快捷键(Finder、浏览器的 Cmd+N 都会被接管),所以热键写进配置可改。

## 侧栏

- 顶部 "ePostit" 标题 + « 收起按钮,收起后侧栏宽度为 0;宽度与展开状态写入配置。
- `Select` 下拉:Sort by → Last edited / Created / Title。
- 列表项显示标题 + 相对时间(如 `2h ago`,tooltip 显示完整时间)。标题允许重复,重名条目靠时间区分。
- 点击条目选中并载入编辑器,焦点留在列表项上;对已选中条目按 Enter 进入改名:原地换成单行 `Input`,预填当前标题并全选,Enter 提交、失焦提交。改名只改写文件内 front matter 的 `title` 字段。
- 选中条目后 Cmd+Backspace 删除笔记,文件移入 `notes/.trash/`。
- 右上角铅笔按钮或聚焦时 Cmd+N 新建笔记:标题 `Untitled`,立刻写文件、打开、焦点进编辑器。

## 存储

笔记目录 `~/Library/Application Support/epostit/notes/`(可配置),每条笔记一个 Markdown 文件:

- 文件名:`<ulid>.md`,id 全局唯一,标题重名不产生冲突
- 文件内容:YAML front matter(`title`、`created`)+ 正文
- 最后编辑时间用文件 mtime,创建时间用 front matter 的 `created`(复制文件后仍然保留)
- 外部放入的缺 front matter 的 `.md` 文件:标题取文件名,扫描时补写 front matter

保存:编辑器 `InputEvent::Change` 后 500ms 防抖写文件,窗口隐藏、切换笔记、退出前强制保存。

## 配置

`~/Library/Application Support/epostit/config.toml`:

```toml
hotkey = "cmd-n"          # 全局唤起键
notes_dir = "..."         # 笔记目录
hide_on_blur = true
sidebar_width = 240
sidebar_collapsed = false
sort = "last_edited"
```

## 代码结构

- `main.rs`:入口,`gpui_kit::init`、打开窗口、装托盘、装热键、设置 accessory policy
- `hotkey.rs`:全局热键注册与按聚焦状态注销、channel 通知主线程
- `tray.rs`:NSStatusItem 与菜单
- `store.rs`:笔记扫描、新建、改名、防抖保存
- `keymap.rs`:全部按键绑定,含编辑器增强动作与 Escape
- `config.rs`:配置读写
- `ui/root.rs`:侧栏 + 编辑器布局、Escape 处理
- `ui/sidebar.rs`:排序下拉、笔记列表、收起、改名状态
- `ui/editor.rs`:`EditorState` 创建、切换笔记、行操作动作
