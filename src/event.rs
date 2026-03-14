use crossterm::event::KeyEvent;

#[derive(Debug)]
pub enum AppEvent {
    KeyInput(KeyEvent),
    MouseClick {
        col: u16,
        row: u16,
    },
    MouseDrag {
        col: u16,
        row: u16,
    },
    MouseUp {
        col: u16,
        row: u16,
    },
    PtyOutput {
        pane_id: u32,
        data: Vec<u8>,
    },
    PtyExit {
        pane_id: u32,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    ExternalNotification {
        #[allow(dead_code)]
        title: String,
        #[allow(dead_code)]
        body: String,
        window: Option<usize>,
    },
}
