use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

use crate::config::Config;
use crate::event::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Prefix,
}

#[derive(Debug)]
pub enum Action {
    ForwardToPty(Vec<u8>),
    SplitVertical,
    SplitHorizontal,
    FocusUp,
    FocusDown,
    FocusLeft,
    FocusRight,
    ClosePane,
    ToggleSidebar,
    ResizeUp,
    ResizeDown,
    ResizeLeft,
    ResizeRight,
    ToggleZoom,
    NewWindow,
    SwitchWindow(usize),
    Quit,
    None,
}

/// (KeyModifiers, KeyCode) → Action名 のマッピング
type BindingMap = HashMap<(KeyModifiers, KeyCode), String>;

pub struct InputHandler {
    pub mode: InputMode,
    prefix_key: KeyCode,
    prefix_modifiers: KeyModifiers,
    bindings: BindingMap,
}

fn default_bindings() -> BindingMap {
    let mut m = HashMap::new();
    m.insert(
        (KeyModifiers::NONE, KeyCode::Char('%')),
        "split_vertical".into(),
    );
    m.insert(
        (KeyModifiers::NONE, KeyCode::Char('"')),
        "split_horizontal".into(),
    );
    m.insert(
        (KeyModifiers::NONE, KeyCode::Char('x')),
        "close_pane".into(),
    );
    m.insert(
        (KeyModifiers::NONE, KeyCode::Char('z')),
        "toggle_sidebar".into(),
    );
    m.insert(
        (KeyModifiers::NONE, KeyCode::Char('c')),
        "new_window".into(),
    );
    m.insert(
        (KeyModifiers::NONE, KeyCode::Char('f')),
        "toggle_zoom".into(),
    );
    m.insert((KeyModifiers::NONE, KeyCode::Char('q')), "quit".into());
    m.insert((KeyModifiers::NONE, KeyCode::Up), "focus_up".into());
    m.insert((KeyModifiers::NONE, KeyCode::Down), "focus_down".into());
    m.insert((KeyModifiers::NONE, KeyCode::Left), "focus_left".into());
    m.insert((KeyModifiers::NONE, KeyCode::Right), "focus_right".into());
    m.insert((KeyModifiers::CONTROL, KeyCode::Up), "resize_up".into());
    m.insert((KeyModifiers::CONTROL, KeyCode::Down), "resize_down".into());
    m.insert((KeyModifiers::CONTROL, KeyCode::Left), "resize_left".into());
    m.insert(
        (KeyModifiers::CONTROL, KeyCode::Right),
        "resize_right".into(),
    );
    m
}

fn action_from_name(name: &str) -> Action {
    match name {
        "split_vertical" => Action::SplitVertical,
        "split_horizontal" => Action::SplitHorizontal,
        "close_pane" => Action::ClosePane,
        "toggle_sidebar" => Action::ToggleSidebar,
        "new_window" => Action::NewWindow,
        "quit" => Action::Quit,
        "focus_up" => Action::FocusUp,
        "focus_down" => Action::FocusDown,
        "focus_left" => Action::FocusLeft,
        "focus_right" => Action::FocusRight,
        "resize_up" => Action::ResizeUp,
        "resize_down" => Action::ResizeDown,
        "resize_left" => Action::ResizeLeft,
        "resize_right" => Action::ResizeRight,
        "toggle_zoom" => Action::ToggleZoom,
        _ => Action::None,
    }
}

impl InputHandler {
    pub fn new(config: &Config) -> Self {
        let mut bindings = default_bindings();
        // configのbindingsでデフォルトを上書き
        for (action_name, (mods, key)) in &config.bindings {
            // まず同じaction名の既存バインドを削除
            bindings.retain(|_, v| v != action_name);
            // 新しいバインドを追加
            bindings.insert((*mods, *key), action_name.clone());
        }
        Self {
            mode: InputMode::Normal,
            prefix_key: config.prefix_key,
            prefix_modifiers: config.prefix_modifiers,
            bindings,
        }
    }

    pub fn handle(&mut self, event: &AppEvent) -> Action {
        match event {
            AppEvent::KeyInput(key) => self.handle_key(*key),
            _ => Action::None,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Action {
        match self.mode {
            InputMode::Normal => {
                if key.modifiers == self.prefix_modifiers && key.code == self.prefix_key {
                    self.mode = InputMode::Prefix;
                    return Action::None;
                }
                Action::ForwardToPty(key_to_bytes(key))
            }
            InputMode::Prefix => {
                self.mode = InputMode::Normal;
                // prefix + 数字 → Window切り替え（常に有効）
                if let KeyCode::Char(c @ '1'..='9') = key.code {
                    if key.modifiers == KeyModifiers::NONE {
                        return Action::SwitchWindow((c as usize) - ('1' as usize));
                    }
                }
                // bindingsマップから検索
                if let Some(action_name) = self.bindings.get(&(key.modifiers, key.code)) {
                    action_from_name(action_name)
                } else {
                    Action::None
                }
            }
        }
    }
}

pub fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+a..z → 0x01..0x1a
                let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
                vec![byte]
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                s.as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![127],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;
    use std::collections::HashMap;

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: crossterm::event::KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_event(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
        AppEvent::KeyInput(make_key(code, modifiers))
    }

    fn default_handler() -> InputHandler {
        InputHandler::new(&Config::default())
    }

    #[test]
    fn normal_mode_forwards_regular_keys() {
        let mut handler = default_handler();
        let event = make_event(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::ForwardToPty(ref data) if data == b"a"));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn ctrl_b_enters_prefix_mode() {
        let mut handler = default_handler();
        let event = make_event(KeyCode::Char('b'), KeyModifiers::CONTROL);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::None));
        assert_eq!(handler.mode, InputMode::Prefix);
    }

    #[test]
    fn custom_prefix_ctrl_a() {
        let config = Config {
            prefix_key: KeyCode::Char('a'),
            prefix_modifiers: KeyModifiers::CONTROL,
            bindings: HashMap::new(),
        };
        let mut handler = InputHandler::new(&config);

        // Ctrl-aでprefixモードに入る
        let action = handler.handle(&make_event(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert!(matches!(action, Action::None));
        assert_eq!(handler.mode, InputMode::Prefix);

        // Ctrl-bは通常キーとしてPTYに転送される
        let mut handler = InputHandler::new(&config);
        let action = handler.handle(&make_event(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert!(matches!(action, Action::ForwardToPty(_)));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn prefix_percent_splits_vertical() {
        let mut handler = default_handler();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('%'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::SplitVertical));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn prefix_quote_splits_horizontal() {
        let mut handler = default_handler();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('"'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::SplitHorizontal));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn prefix_x_closes_pane() {
        let mut handler = default_handler();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::ClosePane));
    }

    #[test]
    fn prefix_z_toggles_sidebar() {
        let mut handler = default_handler();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('z'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::ToggleSidebar));
    }

    #[test]
    fn prefix_q_quits() {
        let mut handler = default_handler();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('q'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::Quit));
    }

    #[test]
    fn prefix_arrows_move_focus() {
        let cases = [
            (KeyCode::Up, Action::FocusUp),
            (KeyCode::Down, Action::FocusDown),
            (KeyCode::Left, Action::FocusLeft),
            (KeyCode::Right, Action::FocusRight),
        ];
        for (code, expected) in cases {
            let mut handler = default_handler();
            handler.mode = InputMode::Prefix;
            let event = make_event(code, KeyModifiers::NONE);
            let action = handler.handle(&event);
            assert_eq!(
                std::mem::discriminant(&action),
                std::mem::discriminant(&expected)
            );
            assert_eq!(handler.mode, InputMode::Normal);
        }
    }

    #[test]
    fn prefix_ctrl_arrows_resize() {
        let cases = [
            (KeyCode::Up, Action::ResizeUp),
            (KeyCode::Down, Action::ResizeDown),
            (KeyCode::Left, Action::ResizeLeft),
            (KeyCode::Right, Action::ResizeRight),
        ];
        for (code, expected) in cases {
            let mut handler = default_handler();
            handler.mode = InputMode::Prefix;
            let event = make_event(code, KeyModifiers::CONTROL);
            let action = handler.handle(&event);
            assert_eq!(
                std::mem::discriminant(&action),
                std::mem::discriminant(&expected)
            );
        }
    }

    #[test]
    fn prefix_unknown_key_returns_none_and_exits_prefix() {
        let mut handler = default_handler();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('?'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::None));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn non_key_event_returns_none() {
        let mut handler = default_handler();
        let event = AppEvent::Resize { cols: 80, rows: 24 };
        let action = handler.handle(&event);
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn key_to_bytes_regular_char() {
        let key = make_key(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(key_to_bytes(key), b"x");
    }

    #[test]
    fn key_to_bytes_ctrl_char() {
        let key = make_key(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(key_to_bytes(key), vec![1]);
        let key = make_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_to_bytes(key), vec![3]);
    }

    #[test]
    fn key_to_bytes_special_keys() {
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Enter, KeyModifiers::NONE)),
            vec![b'\r']
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Backspace, KeyModifiers::NONE)),
            vec![127]
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Tab, KeyModifiers::NONE)),
            vec![b'\t']
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Esc, KeyModifiers::NONE)),
            vec![0x1b]
        );
    }

    #[test]
    fn key_to_bytes_arrow_keys() {
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Up, KeyModifiers::NONE)),
            b"\x1b[A"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Down, KeyModifiers::NONE)),
            b"\x1b[B"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Right, KeyModifiers::NONE)),
            b"\x1b[C"
        );
        assert_eq!(
            key_to_bytes(make_key(KeyCode::Left, KeyModifiers::NONE)),
            b"\x1b[D"
        );
    }

    #[test]
    fn full_sequence_ctrl_b_then_percent() {
        let mut handler = default_handler();

        let action = handler.handle(&make_event(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert!(matches!(action, Action::None));
        assert_eq!(handler.mode, InputMode::Prefix);

        let action = handler.handle(&make_event(KeyCode::Char('%'), KeyModifiers::NONE));
        assert!(matches!(action, Action::SplitVertical));
        assert_eq!(handler.mode, InputMode::Normal);

        let action = handler.handle(&make_event(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(matches!(action, Action::ForwardToPty(ref data) if data == b"l"));
    }

    #[test]
    fn prefix_c_creates_new_window() {
        let mut handler = default_handler();
        handler.mode = InputMode::Prefix;
        let action = handler.handle(&make_event(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(matches!(action, Action::NewWindow));
    }

    #[test]
    fn prefix_number_switches_window() {
        for (ch, expected_idx) in [('1', 0), ('2', 1), ('9', 8)] {
            let mut handler = default_handler();
            handler.mode = InputMode::Prefix;
            let action = handler.handle(&make_event(KeyCode::Char(ch), KeyModifiers::NONE));
            assert!(
                matches!(action, Action::SwitchWindow(idx) if idx == expected_idx),
                "expected SwitchWindow({}) for '{}'",
                expected_idx,
                ch
            );
        }
    }

    #[test]
    fn custom_bindings_override_defaults() {
        let mut bindings = HashMap::new();
        // split_verticalを'v'に変更
        bindings.insert(
            "split_vertical".to_string(),
            (KeyModifiers::NONE, KeyCode::Char('v')),
        );
        let config = Config {
            prefix_key: KeyCode::Char('b'),
            prefix_modifiers: KeyModifiers::CONTROL,
            bindings,
        };
        let mut handler = InputHandler::new(&config);
        handler.mode = InputMode::Prefix;

        // 'v'でsplit_verticalが発火する
        let action = handler.handle(&make_event(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(matches!(action, Action::SplitVertical));

        // 元の'%'はもう効かない
        handler.mode = InputMode::Prefix;
        let action = handler.handle(&make_event(KeyCode::Char('%'), KeyModifiers::NONE));
        assert!(matches!(action, Action::None));
    }
}
