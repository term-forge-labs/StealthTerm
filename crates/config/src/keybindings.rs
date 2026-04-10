use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyBinding {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Meta,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    NewLocalTab,
    NewSshConnection,
    CloseTab,
    NextTab,
    PrevTab,
    ToggleSidebar,
    CommandPalette,
    SearchOutput,
    SplitHorizontal,
    SplitVertical,
    SendInput,
    Copy,
    Paste,
    FontIncrease,
    FontDecrease,
    FontReset,
    Fullscreen,
    ToggleSftpPanel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    pub bindings: HashMap<Action, KeyBinding>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = HashMap::new();

        let kb = |mods: &[Modifier], k: &str| KeyBinding {
            modifiers: mods.to_vec(),
            key: k.to_string(),
        };

        bindings.insert(Action::NewLocalTab, kb(&[Modifier::Ctrl], "t"));
        bindings.insert(Action::NewSshConnection, kb(&[Modifier::Ctrl], "n"));
        bindings.insert(Action::CloseTab, kb(&[Modifier::Ctrl], "w"));
        bindings.insert(Action::NextTab, kb(&[Modifier::Ctrl], "Tab"));
        bindings.insert(Action::PrevTab, kb(&[Modifier::Ctrl, Modifier::Shift], "Tab"));
        bindings.insert(Action::ToggleSidebar, kb(&[Modifier::Ctrl], "b"));
        bindings.insert(Action::CommandPalette, kb(&[Modifier::Ctrl, Modifier::Shift], "p"));
        bindings.insert(Action::SearchOutput, kb(&[Modifier::Ctrl, Modifier::Shift], "f"));
        bindings.insert(Action::SplitHorizontal, kb(&[Modifier::Ctrl, Modifier::Shift], "d"));
        bindings.insert(Action::SplitVertical, kb(&[Modifier::Ctrl, Modifier::Shift], "r"));
        bindings.insert(Action::SendInput, kb(&[Modifier::Ctrl], "Return"));
        bindings.insert(Action::Copy, kb(&[Modifier::Ctrl, Modifier::Shift], "c"));
        bindings.insert(Action::Paste, kb(&[Modifier::Ctrl, Modifier::Shift], "v"));
        bindings.insert(Action::FontIncrease, kb(&[Modifier::Ctrl], "="));
        bindings.insert(Action::FontDecrease, kb(&[Modifier::Ctrl], "-"));
        bindings.insert(Action::FontReset, kb(&[Modifier::Ctrl], "0"));
        bindings.insert(Action::Fullscreen, kb(&[], "F11"));

        Self { bindings }
    }
}
