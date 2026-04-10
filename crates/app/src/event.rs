use stealthterm_config::connections::ConnectionConfig;

/// Application-wide events dispatched between UI components and backend
#[derive(Debug, Clone)]
pub enum AppEvent {
    // Tab management
    NewLocalTab,
    #[allow(dead_code)]
    NewSshTab(ConnectionConfig),
    CloseTab(String),
    CloseAllTabs,
    CloseOtherTabs(String),
    CloseTabsToTheRight(String),
    CloseTabsToTheLeft(String),
    ActivateTab(String),
    NextTab,
    PrevTab,

    // Pane management
    SplitHorizontal,
    SplitVertical,

    // Sidebar
    ToggleSidebar,
    OpenConnection(String),
    NewConnection,
    NewConnectionInGroup(String),
    EditConnection(String),
    DeleteConnection(String),
    DuplicateSshTab(String),
    PasteConnection,

    // Search
    ToggleSearch,

    // SFTP
    ToggleSftp,

    // Batch
    ToggleBatchMode,

    // Local terminal
    OpenLocalTerminal,

    // Font
    FontIncrease,
    FontDecrease,
    FontReset,

    // Window
    Fullscreen,

    // Command palette
    CommandPalette,

    // Settings
    OpenSettings,
    ToggleMainMenu,
    ShowAbout,
}
