//! Single-line native input, adapted from GPUI `examples/input.rs`.
use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div, fill,
    point, prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use super::super::{SettingsApp, style::palette};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct NumericInput {
    pub min: u64,
    pub max: u64,
}

#[derive(Clone, Default)]
pub(crate) struct TextInputOptions {
    /// Display the value without accepting focus, selection, keyboard or IME input.
    pub disabled: bool,
    pub placeholder: SharedString,
    /// Maximum Unicode scalar count; selection replacement retains available space.
    pub max_length: Option<usize>,
    /// Accept ASCII digits, clamp on Enter/blur (empty commits to min).
    pub numeric: Option<NumericInput>,
    /// Center the text while it fits (stepper values).
    pub centered: bool,
    /// Optional caller overrides; the shell supplies all other text/layout styles.
    pub placeholder_color: Option<gpui::Hsla>,
    pub caret_color: Option<gpui::Hsla>,
    pub selection_color: Option<gpui::Hsla>,
}

#[derive(Clone, Debug)]
pub(crate) enum TextInputEvent {
    Change(SharedString),
    Focus,
    Blur(SharedString),
    Enter(SharedString),
    Escape,
    Up,
    Down,
}

struct InputKeys;
impl gpui::Global for InputKeys {}

const CARET_WIDTH: Pixels = px(2.);
const LINE_BREAKS: [char; 4] = ['\r', '\n', '\u{2028}', '\u{2029}'];

fn single_line(text: SharedString) -> SharedString {
    if text.contains(LINE_BREAKS) {
        text.replace("\r\n", "\n").replace(LINE_BREAKS, " ").into()
    } else {
        text
    }
}

/// Persistent input entity. Call during render with a stable id and the current
/// backing value. All events have the standard settings callback shape.
///
/// ```ignore
/// let input = text_input("rename", name, TextInputOptions::default(), window, cx,
///     |app, event, _, cx| if let TextInputEvent::Change(value) = event {
///         app.update_config(cx, |c| { /* apply value */ });
///     });
/// text_input_container("rename-shell", &input).h(px(38.)).px(px(12.))
/// ```
pub(crate) fn text_input(
    id: impl Into<ElementId>,
    value: impl Into<SharedString>,
    mut options: TextInputOptions,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_event: impl Fn(&mut SettingsApp, TextInputEvent, &mut Window, &mut Context<SettingsApp>)
    + 'static,
) -> Entity<TextInput> {
    if !cx.has_global::<InputKeys>() {
        cx.set_global(InputKeys);
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some("SettingsTextInput")),
            KeyBinding::new("shift-backspace", Backspace, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-backspace", BackspaceWord, Some("SettingsTextInput")),
            KeyBinding::new("delete", Delete, Some("SettingsTextInput")),
            KeyBinding::new("left", Left, Some("SettingsTextInput")),
            KeyBinding::new("right", Right, Some("SettingsTextInput")),
            KeyBinding::new("shift-left", SelectLeft, Some("SettingsTextInput")),
            KeyBinding::new("shift-right", SelectRight, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-a", SelectAll, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-v", Paste, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-c", Copy, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-x", Cut, Some("SettingsTextInput")),
            KeyBinding::new("home", Home, Some("SettingsTextInput")),
            KeyBinding::new("end", End, Some("SettingsTextInput")),
            KeyBinding::new("shift-home", SelectHome, Some("SettingsTextInput")),
            KeyBinding::new("shift-end", SelectEnd, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-left", WordLeft, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-right", WordRight, Some("SettingsTextInput")),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("SettingsTextInput")),
            KeyBinding::new(
                "ctrl-shift-right",
                SelectWordRight,
                Some("SettingsTextInput"),
            ),
        ]);
    }
    let app = cx.weak_entity();
    let callback: Rc<dyn Fn(TextInputEvent, &mut Window, &mut App)> =
        Rc::new(move |event, window, cx| {
            let focus_changed = matches!(&event, TextInputEvent::Focus | TextInputEvent::Blur(_));
            let _ = app.update(cx, |app, cx| {
                on_event(app, event, window, cx);
                if focus_changed {
                    cx.notify();
                }
            });
        });
    let value = single_line(value.into());
    options.placeholder = single_line(options.placeholder);
    let initial = value.clone();
    let init_options = options.clone();
    let init_callback = callback.clone();
    let input = window.use_keyed_state(id, cx, move |window, cx| {
        let focus_handle = cx.focus_handle();
        cx.on_focus(
            &focus_handle,
            window,
            |input: &mut TextInput, window, cx| {
                input.blink = Instant::now();
                input.blink_generation += 1;
                let generation = input.blink_generation;
                (input.callback)(TextInputEvent::Focus, window, cx);
                cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(500))
                            .await;
                        let active = this
                            .update_in(cx, |input, window, cx| {
                                let active = input.blink_generation == generation
                                    && input.focus_handle.is_focused(window);
                                if active {
                                    cx.notify();
                                }
                                active
                            })
                            .unwrap_or(false);
                        if !active {
                            break;
                        }
                    }
                })
                .detach();
                cx.notify();
            },
        )
        .detach();
        cx.on_blur(
            &focus_handle,
            window,
            |input: &mut TextInput, window, cx| {
                input.blink_generation += 1;
                input.is_selecting = false;
                input.marked_range = None;
                input.commit(window, cx);
                (input.callback)(TextInputEvent::Blur(input.content.clone()), window, cx);
                cx.notify();
            },
        )
        .detach();
        TextInput {
            focus_handle,
            content: initial.clone(),
            external_value: initial.clone(),
            placeholder: init_options.placeholder.clone(),
            selected_range: initial.len()..initial.len(),
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            options: init_options,
            callback: init_callback,
            scroll_x: px(0.),
            blink: Instant::now(),
            blink_generation: 0,
        }
    });
    input.update(cx, |input, _| {
        if options.disabled && !input.options.disabled {
            input.is_selecting = false;
            input.marked_range = None;
            input.blink_generation += 1;
        }
        input.options = options;
        input.placeholder = input.options.placeholder.clone();
        input.callback = callback;
        if input.external_value != value {
            // Silent external synchronization: do not feed a Change back to config.
            input.external_value = value.clone();
            if input.content != value {
                input.reset_content(value);
            }
        }
    });
    if input.read(cx).options.disabled && input.read(cx).focus_handle.is_focused(window) {
        // Also handles disabling through a live config update, rather than only
        // a click in the section. Blur outside the input's entity borrow.
        window.blur(cx);
    }
    input
}

/// Neutral, caller-styled shell. Typography, background, padding and height
/// belong to the section; the input inherits its text style.
pub(crate) fn text_input_container(
    id: impl Into<ElementId>,
    input: &Entity<TextInput>,
) -> gpui::Stateful<gpui::Div> {
    let click_input = input.clone();
    let blur_input = input.clone();
    div()
        .id(id)
        .min_w_0()
        .flex()
        .items_center()
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            click_input.update(cx, |input, cx| input.on_mouse_down(event, window, cx));
        })
        .on_mouse_down_out(move |event, window, cx| {
            if event.button == MouseButton::Left
                && blur_input.read(cx).focus_handle.is_focused(window)
            {
                window.blur(cx);
            }
        })
        .child(input.clone())
}

actions!(
    text_input,
    [
        Backspace,
        BackspaceWord,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        SelectHome,
        SelectEnd,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        Paste,
        Cut,
        Copy,
    ]
);

pub(crate) struct TextInput {
    options: TextInputOptions,
    callback: Rc<dyn Fn(TextInputEvent, &mut Window, &mut App)>,
    scroll_x: Pixels,
    blink: Instant,
    blink_generation: u64,
    external_value: SharedString,
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    pub fn value(&self) -> SharedString {
        self.content.clone()
    }

    /// Focus and select all from a caller's event callback.
    pub fn focus_and_select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.options.disabled {
            self.focus_handle.focus(window, cx);
            self.select_all(&SelectAll, window, cx);
        }
    }

    /// Focus an inline editor with an unselected caret at the end of its value.
    pub fn focus_at_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.options.disabled {
            self.focus_handle.focus(window, cx);
            self.move_to(self.content.len(), cx);
        }
    }

    pub fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.reset_content(single_line(value.into()));
        cx.notify();
    }

    fn reset_content(&mut self, value: SharedString) {
        self.content = value;
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        self.selection_reversed = false;
        self.scroll_x = px(0.);
        self.blink = Instant::now();
    }

    fn sanitize_insert(&self, text: &str, range: &Range<usize>) -> String {
        let retained =
            self.content[..range.start].chars().count() + self.content[range.end..].chars().count();
        let limit = self
            .options
            .max_length
            .map(|n| n.saturating_sub(retained))
            .unwrap_or(usize::MAX);
        text.chars()
            .filter(|ch| {
                !ch.is_control()
                    && !matches!(ch, '\u{2028}' | '\u{2029}')
                    && (self.options.numeric.is_none() || ch.is_ascii_digit())
            })
            .take(limit)
            .collect()
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.options.disabled {
            return;
        }
        if let Some(numeric) = self.options.numeric {
            let value = if self.content.is_empty() {
                numeric.min
            } else {
                self.content.parse::<u64>().unwrap_or(u64::MAX)
            };
            let value: SharedString = value
                .clamp(numeric.min, numeric.max.max(numeric.min))
                .to_string()
                .into();
            if value != self.content {
                self.set_value(value, cx);
                (self.callback)(TextInputEvent::Change(self.content.clone()), window, cx);
            }
        }
    }
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.previous_word_start(), cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.next_word_start(), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_word_start(), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_word_start(), cx);
    }

    fn previous_word_start(&self) -> usize {
        self.content
            .unicode_word_indices()
            .take_while(|(index, _)| *index < self.cursor_offset())
            .last()
            .map_or(0, |(index, _)| index)
    }

    fn next_word_start(&self) -> usize {
        self.content
            .unicode_word_indices()
            .find(|(index, _)| *index > self.cursor_offset())
            .map_or(self.content.len(), |(index, _)| index)
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_back_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn backspace_word(&mut self, _: &BackspaceWord, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_back_to(self.previous_word_start(), window, cx);
    }

    fn delete_back_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            if self.cursor_offset() == offset {
                window.play_system_bell();
                return;
            }
            self.select_to(offset, cx);
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                window.play_system_bell();
                return;
            }
            self.select_to(next, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.options.disabled {
            return;
        }
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);

        self.is_selecting = event.click_count == 1;
        let index = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(index, cx);
        } else if event.click_count >= 3 {
            self.select_all_text(cx);
        } else if event.click_count == 2 {
            let range = self.word_at(index);
            self.move_to(range.start, cx);
            self.select_to(range.end, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn word_at(&self, offset: usize) -> Range<usize> {
        if self.content.is_empty() {
            return 0..0;
        }
        let offset = offset.min(self.content.len() - 1);
        self.content
            .split_word_bound_indices()
            .find_map(|(start, text)| {
                let end = start + text.len();
                (start <= offset && offset < end).then_some(start..end)
            })
            .unwrap_or(offset..offset)
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = single_line(text.into()).replace('\t', " ");
            if !text.is_empty() {
                self.replace_text_in_range(None, &text, window, cx);
            }
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selection_reversed = false;
        self.blink = Instant::now();
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if line.text != self.content {
            return self.cursor_offset();
        }
        let x = position.x - bounds.left() + self.scroll_x;
        let index = line.closest_index_for_x(x);
        // Text shaping can return an index inside a multi-code-point grapheme.
        if index >= self.content.len() {
            let last = self.previous_boundary(self.content.len());
            if x < (line.x_for_index(last) + line.width()) / 2. {
                return last;
            }
            return self.content.len();
        }
        self.previous_boundary(index + 1)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.blink = Instant::now();
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.options.disabled && !ignore_disabled_input {
            return None;
        }
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.options.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let sanitized = self.sanitize_insert(new_text, &range);
        if !new_text.is_empty() && sanitized.is_empty() && self.marked_range.is_none() {
            return;
        }
        let content: SharedString =
            (self.content[..range.start].to_owned() + &sanitized + &self.content[range.end..])
                .into();
        let changed = content != self.content;
        self.content = content;
        let cursor = range.start + sanitized.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range.take();
        self.blink = Instant::now();
        if changed {
            (self.callback)(TextInputEvent::Change(self.content.clone()), window, cx);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.options.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let new_text = self.sanitize_insert(new_text, &range);
        self.content =
            (self.content[..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        let offset = |n: usize| {
            let mut units = 0;
            let mut bytes = 0;
            for ch in new_text.chars() {
                if units + ch.len_utf16() > n {
                    break;
                }
                units += ch.len_utf16();
                bytes += ch.len_utf8();
            }
            range.start + bytes
        };
        self.selected_range = new_selected_range_utf16
            .map(|r| offset(r.start)..offset(r.end))
            .unwrap_or(range.start + new_text.len()..range.start + new_text.len());
        self.selection_reversed = false;
        self.blink = Instant::now();
        (self.callback)(TextInputEvent::Change(self.content.clone()), _window, cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() - self.scroll_x + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() - self.scroll_x + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        bounds.localize(&point)?;
        if self.content.is_empty() {
            return Some(0);
        }
        let last_layout = self.last_layout.as_ref()?;
        if last_layout.text != self.content {
            return None;
        }

        let utf8_index = last_layout.closest_index_for_x(point.x - bounds.left() + self.scroll_x);
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    scroll_x: Pixels,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ShapedLine;
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let text_style = window.text_style();

        let (display_text, text_color) = if input.content.is_empty() {
            (
                input.placeholder.clone(),
                input
                    .options
                    .placeholder_color
                    .unwrap_or(palette(cx).muted.hsla()),
            )
        } else {
            (input.content.clone(), text_style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: text_style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        // A caller's line height can be shorter than the shaped font metrics.
        let line_height = window
            .line_height()
            .max(line.ascent + line.descent + px(2.));
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = line_height.into();
        (window.request_layout(style, [], cx), line)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        line: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let selected_range = input.selected_range.clone();
        let cursor_pos = if input.content.is_empty() {
            px(0.)
        } else {
            line.x_for_index(input.cursor_offset())
        };
        let caret_color = input
            .options
            .caret_color
            .unwrap_or(window.text_style().color);
        let selection_color = input
            .options
            .selection_color
            .unwrap_or(palette(cx).accent.opacity(0.3).hsla());
        let max_scroll = (line.width() - bounds.size.width + CARET_WIDTH).max(px(0.));
        // A negative scroll centers a line that fits; hit testing and the
        // caret already subtract it.
        let centered = input.options.centered
            && line.width() + CARET_WIDTH <= bounds.size.width;
        let scroll_x = if centered {
            -(bounds.size.width - line.width()) / 2.
        } else if !input.content.is_empty() && input.focus_handle.is_focused(window) {
            input
                .scroll_x
                .max(cursor_pos - bounds.size.width + CARET_WIDTH)
                .min(cursor_pos)
                .clamp(px(0.), max_scroll)
        } else {
            px(0.)
        };
        let caret_visible = input.blink.elapsed().as_millis() % 1000 < 500;
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() - scroll_x + cursor_pos, bounds.top()),
                        size(CARET_WIDTH, bounds.size.height),
                    ),
                    caret_color,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() - scroll_x + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() - scroll_x + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    selection_color,
                )),
                None,
            )
        };
        PrepaintState {
            scroll_x,
            cursor: if caret_visible { cursor } else { None },
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        line: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        let disabled = self.input.read(cx).options.disabled;
        if !disabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }
        let focused = !disabled && focus_handle.is_focused(window);
        let scroll_x = prepaint.scroll_x;
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            if focused && let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection)
            }
            line.paint(
                point(bounds.left() - scroll_x, bounds.top()),
                bounds.size.height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .unwrap();

            if focused && let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        });
        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line.clone());
            input.last_bounds = Some(bounds);
            input.scroll_x = scroll_x;
        });
        let input = self.input.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == gpui::DispatchPhase::Bubble && input.read(cx).is_selecting {
                input.update(cx, |input, cx| {
                    if event.pressed_button == Some(MouseButton::Left) {
                        input.on_mouse_move(event, window, cx);
                    } else {
                        input.is_selecting = false;
                    }
                });
            }
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .when(self.options.disabled, |this| this.cursor_not_allowed())
            .when(!self.options.disabled, |this| {
                this.key_context("SettingsTextInput")
                    .track_focus(&self.focus_handle)
                    .cursor(CursorStyle::IBeam)
                    .on_action(cx.listener(Self::backspace))
                    .on_action(cx.listener(Self::backspace_word))
                    .on_action(cx.listener(Self::delete))
                    .on_action(cx.listener(Self::left))
                    .on_action(cx.listener(Self::right))
                    .on_action(cx.listener(Self::select_left))
                    .on_action(cx.listener(Self::select_right))
                    .on_action(cx.listener(Self::select_all))
                    .on_action(cx.listener(Self::home))
                    .on_action(cx.listener(Self::end))
                    .on_action(cx.listener(Self::select_home))
                    .on_action(cx.listener(Self::select_end))
                    .on_action(cx.listener(Self::word_left))
                    .on_action(cx.listener(Self::word_right))
                    .on_action(cx.listener(Self::select_word_left))
                    .on_action(cx.listener(Self::select_word_right))
                    .on_action(cx.listener(Self::paste))
                    .on_action(cx.listener(Self::cut))
                    .on_action(cx.listener(Self::copy))
                    .on_key_down(
                        cx.listener(|input, event: &gpui::KeyDownEvent, window, cx| {
                            let event = match event.keystroke.key.as_str() {
                                "enter" => {
                                    input.commit(window, cx);
                                    Some(TextInputEvent::Enter(input.content.clone()))
                                }
                                "escape" => Some(TextInputEvent::Escape),
                                "up" if input.options.numeric.is_some() => Some(TextInputEvent::Up),
                                "down" if input.options.numeric.is_some() => {
                                    Some(TextInputEvent::Down)
                                }
                                _ => None,
                            };
                            if let Some(event) = event {
                                (input.callback)(event, window, cx);
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            })
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
