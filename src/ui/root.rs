use crate::config::{Config, SortMode};
use crate::hotkey;
use crate::keymap::*;
use crate::store::{Note, Store};
use chrono::{DateTime, Local};
use gpui_kit::base::IndexPath;
use gpui_kit::base::input::{InputBaseState, InputModeKind};
use gpui_kit::component::Sizable;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::input::{
    AnyInputState, Editor, EditorState, Input, InputEvent, InputState,
};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, Icon, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::ops::Range;
use std::time::Duration;

const EDIT_SVG: &[u8] = include_bytes!("../../assets/icons/edit.svg");
const BREADCRUMBS_SVG: &[u8] = include_bytes!("../../assets/icons/breadcrumbs.svg");
const BREADCRUMBS_LEFT_SVG: &[u8] = include_bytes!("../../assets/icons/breadcrumbs-left.svg");
const ARCHIVE_SVG: &[u8] = include_bytes!("../../assets/icons/archive.svg");
const UNARCHIVE_SVG: &[u8] = include_bytes!("../../assets/icons/unarchive.svg");
const DELETE_SVG: &[u8] = include_bytes!("../../assets/icons/delete.svg");
const RAIL_WIDTH: f32 = 48.;

pub struct RootView {
    list_focus: FocusHandle,
    store: Store,
    notes: Vec<Note>,
    current: Option<String>,
    editor: Entity<EditorState>,
    sort: Entity<SelectState<Vec<SharedString>>>,
    sort_mode: SortMode,
    collapsed: bool,
    renaming: Option<String>,
    rename_input: Option<Entity<InputState>>,
    show_archived: bool,
    hovered_row: Option<String>,
    dirty: bool,
    save_gen: u64,
    window_active: bool,
    hiding: bool,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for RootView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.list_focus.clone()
    }
}

impl RootView {
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .soft_wrap(true)
                .searchable(true)
                .placeholder("Start writing…")
        });
        let sort = cx.new(|cx| {
            SelectState::new(
                SortMode::LABELS.map(SharedString::from).to_vec(),
                Some(IndexPath::default().row(sort_index(cx.global::<Config>().sort))),
                window,
                cx,
            )
        });

        let subscriptions = vec![
            cx.subscribe_in(&editor, window, |this, _, ev, window, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.on_editor_change(window, cx);
                }
            }),
            cx.subscribe_in(&sort, window, |this, _, ev, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = ev {
                    this.set_sort(SortMode::from_label(value.as_ref()), cx);
                }
            }),
        ];

        let mut this = Self {
            list_focus: cx.focus_handle(),
            store,
            notes: Vec::new(),
            current: None,
            editor,
            sort,
            sort_mode: cx.global::<Config>().sort,
            collapsed: cx.global::<Config>().sidebar_collapsed,
            renaming: None,
            rename_input: None,
            show_archived: false,
            hovered_row: None,
            dirty: false,
            save_gen: 0,
            window_active: window.is_window_active(),
            hiding: false,
            _subscriptions: subscriptions,
        };
        this.notes = this.store.scan();
        this.sort_notes();
        let first = this.notes.first().map(|n| n.id.clone());
        match first {
            Some(id) => this.open_note(&id, window, cx),
            None => this.create_note(window, cx),
        }
        let editor = this.editor.clone();
        window.defer(cx, move |window, cx| {
            let focus = editor.read(cx).focus_handle(cx).clone();
            focus.focus(window, cx);
        });
        this
    }

    /// 激活状态变化在 render 里轮询:聚焦注销全局热键,失焦注册并按需隐藏
    fn check_activation(&mut self, window: &Window, cx: &mut Context<Self>) {
        let active = window.is_window_active();
        if active == self.window_active || self.hiding {
            return;
        }
        self.window_active = active;
        if active {
            hotkey::unregister();
        } else {
            hotkey::register();
            if cx.global::<Config>().hide_on_blur {
                self.hiding = true;
                self.flush_save(cx);
                crate::request_hide(cx);
                self.hiding = false;
            }
        }
    }

    fn note(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    fn sort_notes(&mut self) {
        match self.sort_mode {
            SortMode::LastEdited => self.notes.sort_by(|a, b| b.modified.cmp(&a.modified)),
            SortMode::Created => self.notes.sort_by(|a, b| b.created.cmp(&a.created)),
            SortMode::Title => self.notes.sort_by(|a, b| a.title.cmp(&b.title)),
        }
    }

    /// 切换笔记:先保存当前,再载入目标正文
    fn open_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.current.as_deref() == Some(id) {
            return;
        }
        self.flush_save(cx);
        let note = self.note(id).expect("open note");
        let body = self.store.body(note);
        self.editor
            .update(cx, |s, cx| s.set_value(body, window, cx));
        self.current = Some(id.to_string());
        self.dirty = false;
        cx.notify();
    }

    pub fn create_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_save(cx);
        let note = self.store.create();
        let id = note.id.clone();
        self.notes.push(note);
        self.sort_notes();
        self.editor.update(cx, |s, cx| s.set_value("", window, cx));
        self.current = Some(id);
        self.dirty = false;
        self.focus_editor(window, cx);
        cx.notify();
    }

    /// 把当前正文写回文件并刷新排序
    pub fn flush_save(&mut self, cx: &mut App) {
        if !self.dirty {
            return;
        }
        let Some(id) = self.current.clone() else {
            return;
        };
        let body = self.editor.read(cx).value().to_string();
        let store = &self.store;
        let note = self
            .notes
            .iter_mut()
            .find(|n| n.id == id)
            .expect("save note");
        store.save(note, &body);
        self.dirty = false;
        self.sort_notes();
    }

    fn on_editor_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dirty = true;
        self.save_gen += 1;
        let generation = self.save_gen;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            this.update_in(cx, |this, _, cx| {
                if this.save_gen == generation && this.dirty {
                    this.flush_save(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn set_sort(&mut self, mode: SortMode, cx: &mut Context<Self>) {
        self.sort_mode = mode;
        cx.global_mut::<Config>().sort = mode;
        cx.global::<Config>().save();
        self.sort_notes();
        cx.notify();
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.hiding = true;
        self.renaming = None;
        self.rename_input = None;
        self.flush_save(cx);
        crate::request_hide(cx);
        self.hiding = false;
    }

    pub fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.editor.read(cx).focus_handle(cx).clone();
        focus.focus(window, cx);
    }

    fn begin_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 改名输入框仍在 NoteList 上下文里,提交时的 Enter 会再次命中
        // enter -> RenameNote 绑定,已在改名中时忽略
        if self.renaming.is_some() {
            return;
        }
        let Some(id) = self.current.clone() else {
            return;
        };
        if self.note(&id).is_some_and(|n| n.archived) {
            return;
        }
        let title = self.note(&id).expect("rename note").title.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title));
        input.update(cx, |s, cx| s.select_all(window, cx));
        let focus = input.read(cx).focus_handle(cx).clone();
        window.defer(cx, move |window, cx| focus.focus(window, cx));
        cx.subscribe_in(&input, window, |this, _, ev, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                this.commit_rename(window, cx);
            }
        })
        .detach();
        self.renaming = Some(id);
        self.rename_input = Some(input);
        cx.notify();
    }

    /// 退出改名状态,标题保持不变
    fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.is_none() {
            return;
        }
        self.renaming = None;
        self.rename_input = None;
        self.list_focus.focus(window, cx);
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.renaming.take() else {
            return;
        };
        let title = self
            .rename_input
            .as_ref()
            .map(|i| i.read(cx).value().trim().to_string())
            .unwrap_or_default();
        if !title.is_empty() {
            let store = &self.store;
            let note = self
                .notes
                .iter_mut()
                .find(|n| n.id == id)
                .expect("commit rename");
            store.rename(note, &title);
        }
        self.rename_input = None;
        self.sort_notes();
        self.list_focus.focus(window, cx);
        cx.notify();
    }

    /// 归档切换只改写 front matter;当前笔记被归档后编辑器锁定
    fn toggle_archive(&mut self, id: &str, cx: &mut Context<Self>) {
        let store = &self.store;
        let note = self
            .notes
            .iter_mut()
            .find(|n| n.id == id)
            .expect("archive note");
        let archived = !note.archived;
        store.set_archived(note, archived);
        cx.notify();
    }

    /// 归档笔记直接删除,普通笔记先弹确认框
    fn request_delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.note(id).expect("delete note");
        if note.archived {
            self.delete_note(id, window, cx);
            return;
        }
        let title = note.title.clone();
        let view = cx.entity();
        let id = id.to_string();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            alert
                .title("Delete note?")
                .description(format!("\"{title}\" will be moved to trash."))
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |this, cx| this.delete_note(&id, window, cx));
                    true
                })
        });
    }

    fn delete_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.note(id).expect("delete note").clone();
        self.store.trash(&note);
        self.notes.retain(|n| n.id != id);
        if self.renaming.as_deref() == Some(id) {
            self.renaming = None;
            self.rename_input = None;
        }
        if self.current.as_deref() == Some(id) {
            self.current = None;
            let next = self
                .notes
                .iter()
                .find(|n| !n.archived)
                .map(|n| n.id.clone());
            match next {
                Some(id) => self.open_note(&id, window, cx),
                None => self.create_note(window, cx),
            }
        }
        cx.notify();
    }

    fn toggle_archived_section(&mut self, cx: &mut Context<Self>) {
        self.show_archived = !self.show_archived;
        cx.notify();
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        cx.global_mut::<Config>().sidebar_collapsed = self.collapsed;
        cx.global::<Config>().save();
        cx.notify();
    }

    /// 侧栏图标按钮:图标颜色与正文一致,悬停显示底色
    fn icon_button(
        &self,
        id: &'static str,
        svg: &'static [u8],
        on: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = cx.theme();
        div()
            .id(id)
            .p_1()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(t.sidebar_accent))
            .child(Icon::default().data(svg).size_4().text_color(t.foreground))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| on(this, window, cx)),
            )
    }

    /// 条目内悬停显示的操作按钮,点击不向下冒泡到条目
    fn row_button(
        &self,
        id: impl Into<ElementId>,
        svg: &'static [u8],
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = cx.theme();
        div()
            .id(id)
            .p_1()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(t.sidebar_accent))
            .child(Icon::default().data(svg).size_4().text_color(t.foreground))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    on(this, window, cx);
                }),
            )
    }

    /// 对当前聚焦的输入组件执行整行操作
    fn line_op(&mut self, window: &mut Window, cx: &mut Context<Self>, op: LineOp) {
        let Some(input) = window.focused_input(cx) else {
            return;
        };
        match input {
            AnyInputState::Editor(e) => e.update(cx, |s, cx| run_line_op(s, window, cx, op)),
            AnyInputState::Textarea(e) => e.update(cx, |s, cx| run_line_op(s, window, cx, op)),
            AnyInputState::Input(e) => e.update(cx, |s, cx| run_line_op(s, window, cx, op)),
            AnyInputState::Otp(_) => {}
        }
    }

    fn render_note_row(
        &self,
        ix: usize,
        note: &Note,
        cx: &Context<Self>,
    ) -> impl IntoElement + 'static {
        let t = cx.theme();
        let id = note.id.clone();
        let archived = note.archived;
        let hovered = self.hovered_row.as_deref() == Some(note.id.as_str());
        let selected = self.current.as_deref() == Some(note.id.as_str());
        let renaming = self.renaming.as_deref() == Some(note.id.as_str());
        let rename_input = self.rename_input.clone();
        let hover_id = id.clone();
        let archive_id = id.clone();
        let delete_id = id.clone();
        h_flex()
            .id(("note", ix))
            .px_3()
            .w_full()
            .h(px(54.))
            .flex_shrink_0()
            .items_center()
            .cursor_pointer()
            .when(selected, |this| this.bg(t.list_active))
            .when(!selected, |this| this.hover(|s| s.bg(t.list_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count >= 2 {
                        this.begin_rename(window, cx);
                    } else {
                        this.open_note(&id, window, cx);
                        this.list_focus.focus(window, cx);
                    }
                }),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let was = this.hovered_row.as_deref() == Some(hover_id.as_str());
                if *hovered != was {
                    this.hovered_row = hovered.then(|| hover_id.clone());
                    cx.notify();
                }
            }))
            .map(|this| {
                if renaming {
                    let input = rename_input.unwrap();
                    this.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .child(Input::new(&input).w_full()),
                    )
                } else {
                    this.child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .justify_center()
                            .child(
                                div()
                                    .text_sm()
                                    .w_full()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(note.title.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(t.muted_foreground)
                                    .child(rel_time(note.modified)),
                            ),
                    )
                    .when(hovered, |this| {
                        this.child(
                            h_flex()
                                .flex_shrink_0()
                                .gap_1()
                                .items_center()
                                .child(self.row_button(
                                    ("archive", ix),
                                    if archived { UNARCHIVE_SVG } else { ARCHIVE_SVG },
                                    move |this, _, cx| this.toggle_archive(&archive_id, cx),
                                    cx,
                                ))
                                .child(self.row_button(
                                    ("delete", ix),
                                    DELETE_SVG,
                                    move |this, window, cx| {
                                        this.request_delete(&delete_id, window, cx)
                                    },
                                    cx,
                                )),
                        )
                    })
                }
            })
    }
}

fn sort_index(mode: SortMode) -> usize {
    match mode {
        SortMode::LastEdited => 0,
        SortMode::Created => 1,
        SortMode::Title => 2,
    }
}

fn rel_time(t: DateTime<Local>) -> String {
    let secs = (Local::now() - t).num_seconds().max(0);
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else if secs < 86400 * 30 {
        format!("{}d", secs / 86400)
    } else {
        t.format("%Y-%m-%d").to_string()
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.check_activation(window, cx);
        let t = cx.theme();
        let collapsed = self.collapsed;
        let sidebar_width = cx.global::<Config>().sidebar_width;
        // 展开时指向收起方向,收起时指向展开方向
        let toggle_icon = if collapsed {
            BREADCRUMBS_SVG
        } else {
            BREADCRUMBS_LEFT_SVG
        };

        let sidebar = v_flex()
            .w(px(if collapsed { RAIL_WIDTH } else { sidebar_width }))
            .h_full()
            .flex_shrink_0()
            .bg(t.sidebar)
            .border_r_1()
            .border_color(t.sidebar_border)
            .when(collapsed, |this| {
                this.items_center()
                    .pt_3()
                    .gap_2()
                    .window_control_area(WindowControlArea::Drag)
                    .child(self.icon_button(
                        "new-note",
                        EDIT_SVG,
                        |this, window, cx| this.create_note(window, cx),
                        cx,
                    ))
                    .child(self.icon_button(
                        "toggle-sidebar",
                        toggle_icon,
                        |this, _, cx| this.toggle_sidebar(cx),
                        cx,
                    ))
            })
            .when(!collapsed, |this| {
                this.child(
                    v_flex()
                        .id("note-list")
                        .key_context(LIST_CONTEXT)
                        .track_focus(&self.list_focus)
                        .size_full()
                        .child(
                            h_flex()
                                .id("sidebar-header")
                                .px_3()
                                .py_3()
                                .items_center()
                                .justify_between()
                                .window_control_area(WindowControlArea::Drag)
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("ePostit"),
                                )
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .items_center()
                                        .child(self.icon_button(
                                            "new-note",
                                            EDIT_SVG,
                                            |this, window, cx| this.create_note(window, cx),
                                            cx,
                                        ))
                                        .child(self.icon_button(
                                            "toggle-sidebar",
                                            toggle_icon,
                                            |this, _, cx| this.toggle_sidebar(cx),
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            h_flex()
                                .px_3()
                                .pb_2()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(t.muted_foreground)
                                        .child("Sort by"),
                                )
                                .child(Select::new(&self.sort).small().w(px(128.))),
                        )
                        .child(
                            v_flex()
                                .id("rows")
                                .flex_1()
                                .overflow_y_scrollbar()
                                .children({
                                    let mut rows: Vec<AnyElement> =
                                        Vec::with_capacity(self.notes.len() + 2);
                                    for (ix, note) in
                                        self.notes.iter().enumerate().filter(|(_, n)| !n.archived)
                                    {
                                        rows.push(
                                            self.render_note_row(ix, note, &*cx).into_any_element(),
                                        );
                                    }
                                    let archived: Vec<_> = self
                                        .notes
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, n)| n.archived)
                                        .collect();
                                    if !archived.is_empty() {
                                        // 展开后此行保留,归档条目列在它下面
                                        rows.push(
                                            div()
                                                .id("archived-toggle")
                                                .px_3()
                                                .h(px(36.))
                                                .flex_shrink_0()
                                                .flex()
                                                .items_center()
                                                .cursor_pointer()
                                                .bg(t.sidebar_accent)
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(t.muted_foreground)
                                                        .child(format!(
                                                            "Show {} archived notes",
                                                            archived.len()
                                                        )),
                                                )
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.toggle_archived_section(cx)
                                                    }),
                                                )
                                                .into_any_element(),
                                        );
                                        if self.show_archived {
                                            rows.push(
                                                v_flex()
                                                    .bg(t.sidebar_accent)
                                                    .children(archived.into_iter().map(
                                                        |(ix, note)| {
                                                            self.render_note_row(ix, note, &*cx)
                                                                .into_any_element()
                                                        },
                                                    ))
                                                    .into_any_element(),
                                            );
                                        }
                                    }
                                    rows
                                }),
                        ),
                )
            });

        // Editor 元素每帧把 disabled 写回状态,锁定标记由渲染驱动
        let editor_disabled = self
            .current
            .as_ref()
            .and_then(|id| self.note(id))
            .is_some_and(|n| n.archived);
        let editor_area = div()
            .relative()
            .flex_1()
            .h_full()
            .bg(t.background)
            .child(
                div()
                    .id("drag-strip")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h_8()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                div().h_full().w_full().px_4().pt_4().child(
                    Editor::new(&self.editor)
                        .h_full()
                        .w_full()
                        .bordered(false)
                        .appearance(false)
                        .disabled(editor_disabled)
                        .font(crate::theme::app_font())
                        .text_size(px(15.)),
                ),
            );

        h_flex()
            .key_context(APP_CONTEXT)
            .track_focus(&self.list_focus)
            .size_full()
            .bg(t.background)
            .text_color(t.foreground)
            .font(crate::theme::app_font())
            .on_action(cx.listener(|this, _: &HideApp, window, cx| {
                // 有可退出的状态时先退出状态,下一次 Escape 才隐藏窗口
                if this.renaming.is_some() {
                    this.cancel_rename(window, cx);
                } else {
                    this.hide(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewNote, window, cx| this.create_note(window, cx)))
            .on_action(cx.listener(|this, _: &QuitApp, _, cx| {
                this.flush_save(cx);
                cx.quit();
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(
                cx.listener(|this, _: &RenameNote, window, cx| this.begin_rename(window, cx)),
            )
            .on_action(cx.listener(|this, _: &DeleteNote, window, cx| {
                if let Some(id) = this.current.clone() {
                    this.request_delete(&id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SmartCut, window, cx| {
                this.line_op(window, cx, LineOp::SmartCut)
            }))
            .on_action(cx.listener(|this, _: &DeleteLine, window, cx| {
                this.line_op(window, cx, LineOp::DeleteLine)
            }))
            .on_action(cx.listener(|this, _: &DuplicateLine, window, cx| {
                this.line_op(window, cx, LineOp::DuplicateLine)
            }))
            .on_action(cx.listener(|this, _: &MoveLineUp, window, cx| {
                this.line_op(window, cx, LineOp::MoveLine(true))
            }))
            .on_action(cx.listener(|this, _: &MoveLineDown, window, cx| {
                this.line_op(window, cx, LineOp::MoveLine(false))
            }))
            .child(sidebar)
            .child(editor_area)
    }
}

// ---------- 整行操作 ----------

#[derive(Clone, Copy)]
enum LineOp {
    SmartCut,
    DeleteLine,
    DuplicateLine,
    MoveLine(bool), // true = 上移
}

/// 各行起点的 byte 偏移(行范围含行尾换行符)
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    starts
}

fn line_range(starts: &[usize], text_len: usize, line: usize) -> Range<usize> {
    let start = starts[line];
    let end = starts.get(line + 1).copied().unwrap_or(text_len);
    start..end
}

fn line_of_offset(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
}

fn line_text<'a>(text: &'a str, range: Range<usize>) -> &'a str {
    text[range].trim_end_matches(['\n', '\r'])
}

fn run_line_op<M: InputModeKind>(
    state: &mut InputBaseState<M>,
    window: &mut Window,
    cx: &mut Context<InputBaseState<M>>,
    op: LineOp,
) {
    let text = state.text().to_string();
    let starts = line_starts(&text);
    let len_lines = starts.len();
    let cursor = state.cursor().min(text.len());
    let line = line_of_offset(&text, cursor);
    match op {
        LineOp::SmartCut => {
            if state.selected_range().is_empty() {
                state.set_selected_range(line_range(&starts, text.len(), line), cx);
            }
            let copied = state.selected_text().to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(copied));
            state.replace("", window, cx);
        }
        LineOp::DeleteLine => {
            let mut range = line_range(&starts, text.len(), line);
            if range.end == text.len() && range.start > 0 && line == len_lines - 1 {
                // 最后一行没有换行符,连带吃掉上一行的换行符
                range.start -= 1;
            }
            state.set_selected_range(range, cx);
            state.replace("", window, cx);
        }
        LineOp::DuplicateLine => {
            let range = line_range(&starts, text.len(), line);
            let content = line_text(&text, range.clone());
            let ends_with_nl = range.end < text.len();
            state.set_selected_range(range.end..range.end, cx);
            if ends_with_nl {
                state.insert(format!("{content}\n"), window, cx);
            } else {
                state.insert(format!("\n{content}"), window, cx);
            }
        }
        LineOp::MoveLine(up) => {
            let target = if up {
                line.checked_sub(1)
            } else {
                (line + 1 < len_lines).then_some(line + 1)
            };
            let Some(target) = target else {
                return;
            };
            let cur = line_range(&starts, text.len(), line);
            let tgt = line_range(&starts, text.len(), target);
            let content = line_text(&text, cur.clone()).to_string();
            let target_content = line_text(&text, tgt.clone());
            let offset = cursor - cur.start;
            let (span, new_text, new_cursor) = if up {
                let trailing = if line + 1 < len_lines { "\n" } else { "" };
                (
                    tgt.start..cur.end,
                    format!("{content}\n{target_content}{trailing}"),
                    tgt.start + offset,
                )
            } else {
                let trailing = if target + 1 < len_lines { "\n" } else { "" };
                (
                    cur.start..tgt.end,
                    format!("{target_content}\n{content}{trailing}"),
                    cur.start + target_content.len() + 1 + offset,
                )
            };
            state.set_selected_range(span, cx);
            state.replace(new_text, window, cx);
            state.set_selected_range(new_cursor..new_cursor, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RootView;
    use crate::config::Config;
    use crate::keymap::{
        DeleteLine, DeleteNote, DuplicateLine, MoveLineDown, MoveLineUp, NewNote, RenameNote,
        SmartCut,
    };
    use crate::store::Store;
    use gpui_kit::component::WindowExt;
    use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
    use std::path::PathBuf;
    use std::time::Duration;

    fn notes_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-notes")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn note_files(dir: &PathBuf) -> Vec<(PathBuf, String)> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect();
        files.sort();
        files
            .into_iter()
            .map(|p| (p.clone(), std::fs::read_to_string(&p).unwrap()))
            .collect()
    }

    fn harness<'a>(
        cx: &'a mut TestAppContext,
        dir: &PathBuf,
    ) -> (Entity<RootView>, &'a mut VisualTestContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(cx);
            crate::keymap::bind(cx);
            cx.set_global(Config {
                notes_dir: dir.clone(),
                hide_on_blur: false,
                ..Default::default()
            });
        });
        let dir2 = dir.clone();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
                cx.new(|cx| RootView::new(Store::new(dir2), window, cx))
            })
            .expect("open test window")
        });
        let cx = VisualTestContext::from_window(handle, cx).into_mut();
        cx.cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (view, cx)
    }

    fn set_editor(view: &Entity<RootView>, cx: &mut VisualTestContext, text: &str) {
        let text = text.to_string();
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.editor
                    .update(cx, |s, cx| s.set_value(text, window, cx));
            })
        });
    }

    fn set_cursor(view: &Entity<RootView>, cx: &mut VisualTestContext, offset: usize) {
        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.editor
                    .update(cx, |s, cx| s.set_selected_range(offset..offset, cx));
            })
        });
    }

    fn editor_value(view: &Entity<RootView>, cx: &mut VisualTestContext) -> String {
        cx.update(|_, cx| view.read(cx).editor.read(cx).value().to_string())
    }

    #[test]
    fn typing_saves_after_debounce() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("typing_saves");
        let (_view, cx) = harness(cx, &dir);
        cx.simulate_input("hello headless");
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.cx.run_until_parked();
        let files = note_files(&dir);
        assert_eq!(files.len(), 1);
        assert!(files[0].1.contains("hello headless"));
    }

    #[test]
    fn escape_hides_and_flushes_immediately() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("escape");
        let (_view, cx) = harness(cx, &dir);
        let before = crate::hide_count();
        cx.simulate_input("pending text");
        cx.simulate_keystrokes("escape");
        assert!(crate::hide_count() >= before + 1);
        assert!(note_files(&dir)[0].1.contains("pending text"));
    }

    #[test]
    fn new_note_creates_second_untitled() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("new_note");
        let (view, cx) = harness(cx, &dir);
        cx.dispatch_action(NewNote);
        cx.update(|_, cx| {
            let this = view.read(cx);
            assert_eq!(this.notes.len(), 2);
            assert!(this.notes.iter().all(|n| n.title == "Untitled"));
        });
        assert_eq!(note_files(&dir).len(), 2);
    }

    #[test]
    fn rename_rewrites_front_matter() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("rename_ui");
        let (view, cx) = harness(cx, &dir);
        let path = note_files(&dir)[0].0.clone();
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.list_focus.focus(window, cx));
        });
        cx.dispatch_action(RenameNote);
        cx.cx.run_until_parked();
        cx.update(|_, cx| assert!(view.read(cx).renaming.is_some()));
        cx.simulate_input("Renamed Note");
        cx.simulate_keystrokes("enter");
        cx.cx.run_until_parked();

        let files = note_files(&dir);
        assert_eq!(files[0].0, path);
        assert!(files[0].1.contains("title: Renamed Note"));
        cx.update(|_, cx| assert!(view.read(cx).renaming.is_none()));
    }

    #[test]
    fn escape_while_renaming_cancels_then_hides() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("escape_rename");
        let (view, cx) = harness(cx, &dir);
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.list_focus.focus(window, cx));
        });
        cx.dispatch_action(RenameNote);
        cx.cx.run_until_parked();
        cx.update(|_, cx| assert!(view.read(cx).renaming.is_some()));
        let before = crate::hide_count();
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| assert!(view.read(cx).renaming.is_none()));
        assert_eq!(crate::hide_count(), before);
        cx.simulate_keystrokes("escape");
        assert!(crate::hide_count() >= before + 1);
    }

    #[test]
    fn delete_note_moves_to_trash() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("delete_note");
        let (view, cx) = harness(cx, &dir);
        cx.dispatch_action(NewNote);
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.list_focus.focus(window, cx));
        });
        let removed = cx.update(|_, cx| view.read(cx).current.clone().unwrap());
        // 确认按钮走的就是 delete_note
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.delete_note(&removed, window, cx));
        });
        assert!(!dir.join(format!("{removed}.md")).exists());
        assert!(dir.join(".trash").join(format!("{removed}.md")).exists());
        cx.update(|_, cx| assert_eq!(view.read(cx).notes.len(), 1));
    }

    #[test]
    fn archive_toggle_writes_front_matter() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("archive_toggle");
        let (view, cx) = harness(cx, &dir);
        let id = cx.update(|_, cx| view.read(cx).current.clone().unwrap());
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.toggle_archive(&id, cx));
        });
        cx.update(|_, cx| assert!(view.read(cx).note(&id).unwrap().archived));
        assert!(note_files(&dir)[0].1.contains("archived: true"));
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.toggle_archive(&id, cx));
        });
        cx.update(|_, cx| assert!(!view.read(cx).note(&id).unwrap().archived));
        assert!(note_files(&dir)[0].1.contains("archived: false"));
    }

    #[test]
    fn archived_note_is_readonly() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("archived_readonly");
        let (view, cx) = harness(cx, &dir);
        set_editor(&view, cx, "archived body");
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.dirty = false);
        });
        let id = cx.update(|_, cx| view.read(cx).current.clone().unwrap());
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.toggle_archive(&id, cx));
        });
        // disabled 由 Editor 元素渲染时写回状态
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|_, cx| {
            assert!(!view.read(cx).editor.read(cx).is_editable());
        });
        cx.simulate_input("no edits");
        assert_eq!(editor_value(&view, cx), "archived body");
        // 归档笔记不允许改名
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.begin_rename(window, cx));
        });
        cx.update(|_, cx| assert!(view.read(cx).renaming.is_none()));
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.toggle_archive(&id, cx));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|_, cx| {
            assert!(view.read(cx).editor.read(cx).is_editable());
        });
    }

    #[test]
    fn archived_note_deletes_without_dialog() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("archived_delete");
        let (view, cx) = harness(cx, &dir);
        let id = cx.update(|_, cx| view.read(cx).current.clone().unwrap());
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.toggle_archive(&id, cx));
        });
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.request_delete(&id, window, cx));
        });
        assert!(dir.join(".trash").join(format!("{id}.md")).exists());
        cx.update(|window, cx| assert!(!window.has_active_dialog(cx)));
        cx.update(|_, cx| assert!(view.read(cx).notes.iter().all(|n| n.id != id)));
    }

    #[test]
    fn active_note_delete_opens_dialog() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("active_delete_dialog");
        let (view, cx) = harness(cx, &dir);
        cx.update(|window, cx| {
            view.update(cx, |this, cx| this.list_focus.focus(window, cx));
        });
        let id = cx.update(|_, cx| view.read(cx).current.clone().unwrap());
        cx.dispatch_action(DeleteNote);
        cx.update(|window, cx| assert!(window.has_active_dialog(cx)));
        // 确认前笔记不动
        assert!(dir.join(format!("{id}.md")).exists());
        cx.update(|_, cx| assert!(view.read(cx).note(&id).is_some()));
        cx.update(|window, cx| window.close_all_dialogs(cx));
    }

    #[test]
    fn archived_section_expand_collapse() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("archived_section");
        let (view, cx) = harness(cx, &dir);
        let id = cx.update(|_, cx| view.read(cx).current.clone().unwrap());
        cx.dispatch_action(NewNote);
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.toggle_archive(&id, cx));
        });
        cx.update(|_, cx| {
            let this = view.read(cx);
            assert!(!this.show_archived);
            assert_eq!(this.notes.iter().filter(|n| n.archived).count(), 1);
        });
        cx.update(|_, cx| {
            view.update(cx, |this, cx| this.toggle_archived_section(cx));
        });
        cx.update(|_, cx| {
            assert!(view.read(cx).show_archived);
            view.update(cx, |this, cx| this.toggle_archived_section(cx));
            assert!(!view.read(cx).show_archived);
        });
    }

    #[test]
    fn delete_line() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("delete_line");
        let (view, cx) = harness(cx, &dir);
        set_editor(&view, cx, "one\ntwo\nthree");
        set_cursor(&view, cx, 4);
        cx.dispatch_action(DeleteLine);
        assert_eq!(editor_value(&view, cx), "one\nthree");
    }

    #[test]
    fn duplicate_line() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("duplicate_line");
        let (view, cx) = harness(cx, &dir);
        set_editor(&view, cx, "one\ntwo\nthree");
        set_cursor(&view, cx, 4);
        cx.dispatch_action(DuplicateLine);
        assert_eq!(editor_value(&view, cx), "one\ntwo\ntwo\nthree");
    }

    #[test]
    fn smart_cut_copies_line() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("smart_cut");
        let (view, cx) = harness(cx, &dir);
        set_editor(&view, cx, "one\ntwo\nthree");
        set_cursor(&view, cx, 4);
        cx.dispatch_action(SmartCut);
        assert_eq!(editor_value(&view, cx), "one\nthree");
        let item = cx.read_from_clipboard().expect("clipboard item");
        assert_eq!(item.text(), Some("two\n".to_string()));
    }

    #[test]
    fn move_line_up_and_down() {
        let cx = &mut TestAppContext::single();
        let dir = notes_dir("move_line");
        let (view, cx) = harness(cx, &dir);
        set_editor(&view, cx, "one\ntwo\nthree");
        set_cursor(&view, cx, 4);
        cx.dispatch_action(MoveLineDown);
        assert_eq!(editor_value(&view, cx), "one\nthree\ntwo");
        set_cursor(&view, cx, 10);
        cx.dispatch_action(MoveLineUp);
        assert_eq!(editor_value(&view, cx), "one\ntwo\nthree");
    }
}
