use crossterm::event::{KeyCode, KeyModifiers};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub prefix_key: KeyCode,
    pub prefix_modifiers: KeyModifiers,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix_key: KeyCode::Char('b'),
            prefix_modifiers: KeyModifiers::CONTROL,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str::<ConfigFile>(&contents) {
                Ok(file) => {
                    let mut config = Config::default();
                    if let Some(prefix) = file.prefix {
                        if let Some((mods, key)) = parse_key_binding(&prefix) {
                            config.prefix_modifiers = mods;
                            config.prefix_key = key;
                        }
                    }
                    config
                }
                Err(_) => Config::default(),
            },
            Err(_) => Config::default(),
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("xmux")
        .join("config.toml")
}

/// "C-a", "C-b", "C-Space" のようなキーバインド文字列をパースする
fn parse_key_binding(s: &str) -> Option<(KeyModifiers, KeyCode)> {
    let s = s.trim();
    let parts: Vec<&str> = s.split('-').collect();

    let mut modifiers = KeyModifiers::NONE;
    let key_part;

    match parts.len() {
        1 => {
            key_part = parts[0];
        }
        2 => {
            for m in &parts[..parts.len() - 1] {
                match m.to_uppercase().as_str() {
                    "C" | "CTRL" | "CONTROL" => modifiers |= KeyModifiers::CONTROL,
                    "S" | "SHIFT" => modifiers |= KeyModifiers::SHIFT,
                    "A" | "ALT" | "M" | "META" => modifiers |= KeyModifiers::ALT,
                    _ => return None,
                }
            }
            key_part = parts[parts.len() - 1];
        }
        _ => return None,
    }

    let key = match key_part.to_lowercase().as_str() {
        "space" => KeyCode::Char(' '),
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" | "bs" => KeyCode::Backspace,
        s if s.len() == 1 => KeyCode::Char(s.chars().next().unwrap()),
        _ => return None,
    };

    Some((modifiers, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ctrl_b() {
        let (mods, key) = parse_key_binding("C-b").unwrap();
        assert_eq!(mods, KeyModifiers::CONTROL);
        assert_eq!(key, KeyCode::Char('b'));
    }

    #[test]
    fn parse_ctrl_a() {
        let (mods, key) = parse_key_binding("C-a").unwrap();
        assert_eq!(mods, KeyModifiers::CONTROL);
        assert_eq!(key, KeyCode::Char('a'));
    }

    #[test]
    fn parse_ctrl_space() {
        let (mods, key) = parse_key_binding("C-Space").unwrap();
        assert_eq!(mods, KeyModifiers::CONTROL);
        assert_eq!(key, KeyCode::Char(' '));
    }

    #[test]
    fn parse_alt_a() {
        let (mods, key) = parse_key_binding("A-a").unwrap();
        assert_eq!(mods, KeyModifiers::ALT);
        assert_eq!(key, KeyCode::Char('a'));
    }

    #[test]
    fn parse_single_char_no_modifier() {
        let (mods, key) = parse_key_binding("a").unwrap();
        assert_eq!(mods, KeyModifiers::NONE);
        assert_eq!(key, KeyCode::Char('a'));
    }

    #[test]
    fn parse_escape() {
        let (mods, key) = parse_key_binding("C-Esc").unwrap();
        assert_eq!(mods, KeyModifiers::CONTROL);
        assert_eq!(key, KeyCode::Esc);
    }

    #[test]
    fn parse_ctrl_long_form() {
        let (mods, key) = parse_key_binding("Ctrl-a").unwrap();
        assert_eq!(mods, KeyModifiers::CONTROL);
        assert_eq!(key, KeyCode::Char('a'));
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_key_binding("C-a-b").is_none());
        assert!(parse_key_binding("X-a").is_none());
        assert!(parse_key_binding("C-foobar").is_none());
    }

    #[test]
    fn default_config_is_ctrl_b() {
        let config = Config::default();
        assert_eq!(config.prefix_key, KeyCode::Char('b'));
        assert_eq!(config.prefix_modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn parse_whitespace_trimmed() {
        let (mods, key) = parse_key_binding("  C-a  ").unwrap();
        assert_eq!(mods, KeyModifiers::CONTROL);
        assert_eq!(key, KeyCode::Char('a'));
    }
}
