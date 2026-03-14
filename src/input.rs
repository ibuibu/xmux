use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    Quit,
    None,
}

pub struct InputHandler {
    pub mode: InputMode,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
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
                // Ctrl-b → プレフィックスモードに入る
                if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('b') {
                    self.mode = InputMode::Prefix;
                    return Action::None;
                }
                // それ以外はPTYに転送
                Action::ForwardToPty(key_to_bytes(key))
            }
            InputMode::Prefix => {
                self.mode = InputMode::Normal;
                match key.code {
                    KeyCode::Char('%') => Action::SplitVertical,
                    KeyCode::Char('"') => Action::SplitHorizontal,
                    KeyCode::Char('x') => Action::ClosePane,
                    KeyCode::Char('z') => Action::ToggleSidebar,
                    KeyCode::Char('q') => Action::Quit,
                    KeyCode::Up if key.modifiers == KeyModifiers::NONE => Action::FocusUp,
                    KeyCode::Down if key.modifiers == KeyModifiers::NONE => Action::FocusDown,
                    KeyCode::Left if key.modifiers == KeyModifiers::NONE => Action::FocusLeft,
                    KeyCode::Right if key.modifiers == KeyModifiers::NONE => Action::FocusRight,
                    KeyCode::Up if key.modifiers == KeyModifiers::CONTROL => Action::ResizeUp,
                    KeyCode::Down if key.modifiers == KeyModifiers::CONTROL => Action::ResizeDown,
                    KeyCode::Left if key.modifiers == KeyModifiers::CONTROL => Action::ResizeLeft,
                    KeyCode::Right if key.modifiers == KeyModifiers::CONTROL => Action::ResizeRight,
                    _ => Action::None,
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

    #[test]
    fn normal_mode_forwards_regular_keys() {
        let mut handler = InputHandler::new();
        let event = make_event(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::ForwardToPty(ref data) if data == b"a"));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn ctrl_b_enters_prefix_mode() {
        let mut handler = InputHandler::new();
        let event = make_event(KeyCode::Char('b'), KeyModifiers::CONTROL);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::None));
        assert_eq!(handler.mode, InputMode::Prefix);
    }

    #[test]
    fn prefix_percent_splits_vertical() {
        let mut handler = InputHandler::new();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('%'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::SplitVertical));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn prefix_quote_splits_horizontal() {
        let mut handler = InputHandler::new();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('"'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::SplitHorizontal));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn prefix_x_closes_pane() {
        let mut handler = InputHandler::new();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::ClosePane));
    }

    #[test]
    fn prefix_z_toggles_sidebar() {
        let mut handler = InputHandler::new();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('z'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::ToggleSidebar));
    }

    #[test]
    fn prefix_q_quits() {
        let mut handler = InputHandler::new();
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
            let mut handler = InputHandler::new();
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
            let mut handler = InputHandler::new();
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
        let mut handler = InputHandler::new();
        handler.mode = InputMode::Prefix;
        let event = make_event(KeyCode::Char('?'), KeyModifiers::NONE);
        let action = handler.handle(&event);
        assert!(matches!(action, Action::None));
        assert_eq!(handler.mode, InputMode::Normal);
    }

    #[test]
    fn non_key_event_returns_none() {
        let mut handler = InputHandler::new();
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
        // Ctrl+a → 0x01
        let key = make_key(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(key_to_bytes(key), vec![1]);
        // Ctrl+c → 0x03
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
        let mut handler = InputHandler::new();

        // Ctrl-b
        let action = handler.handle(&make_event(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert!(matches!(action, Action::None));
        assert_eq!(handler.mode, InputMode::Prefix);

        // %
        let action = handler.handle(&make_event(KeyCode::Char('%'), KeyModifiers::NONE));
        assert!(matches!(action, Action::SplitVertical));
        assert_eq!(handler.mode, InputMode::Normal);

        // 次の通常キーはPTYに転送される
        let action = handler.handle(&make_event(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(matches!(action, Action::ForwardToPty(ref data) if data == b"l"));
    }
}
