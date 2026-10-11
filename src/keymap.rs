use gpui_kit::*;

gpui_kit::actions!(
    epostit,
    [
        HideApp,
        NewNote,
        QuitApp,
        ToggleSidebar,
        RenameNote,
        DeleteNote,
        SmartCut,
        DeleteLine,
        DuplicateLine,
        MoveLineUp,
        MoveLineDown,
    ]
);

pub const APP_CONTEXT: &str = "Epostit";
pub const LIST_CONTEXT: &str = "NoteList";

/// 应用自身上下文加上输入/列表/菜单的上下文
const CONTEXTS: [&str; 4] = [APP_CONTEXT, LIST_CONTEXT, "Input", "List"];

pub fn bind(cx: &mut App) {
    for ctx in CONTEXTS {
        cx.bind_keys([
            KeyBinding::new("escape", HideApp, Some(ctx)),
            KeyBinding::new("cmd-n", NewNote, Some(ctx)),
            KeyBinding::new("cmd-q", QuitApp, Some(ctx)),
        ]);
    }
    // 在 Input 上下文覆盖编辑器默认绑定(后注册生效)
    cx.bind_keys([
        KeyBinding::new("cmd-x", SmartCut, Some("Input")),
        KeyBinding::new("cmd-shift-k", DeleteLine, Some("Input")),
        KeyBinding::new("cmd-shift-d", DuplicateLine, Some("Input")),
        KeyBinding::new("alt-up", MoveLineUp, Some("Input")),
        KeyBinding::new("alt-down", MoveLineDown, Some("Input")),
    ]);
    // 侧栏列表焦点下的动作
    cx.bind_keys([
        KeyBinding::new("enter", RenameNote, Some(LIST_CONTEXT)),
        KeyBinding::new("cmd-backspace", DeleteNote, Some(LIST_CONTEXT)),
    ]);
    cx.bind_keys([KeyBinding::new("cmd-b", ToggleSidebar, Some(APP_CONTEXT))]);
}
