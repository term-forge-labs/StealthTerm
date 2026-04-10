use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize as NativePtySize, PtySystem, Child};
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use thiserror::Error;
use tracing::{debug, error};

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("PTY creation failed: {0}")]
    Create(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Process spawn failed: {0}")]
    Spawn(String),
}

#[derive(Debug, Clone, Copy)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// Manages a local PTY session
pub struct PtySession {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub size: PtySize,
    // Keep master to maintain ConPTY lifetime (Inner holds HPCON on Windows)
    // If master is dropped, ClosePseudoConsole is called and the child's pseudo-console is closed
    pub master: Arc<std::sync::Mutex<Box<dyn MasterPty + Send>>>,
    // Keep child process handle to prevent it from being closed prematurely
    _child: Box<dyn Child + Send + Sync>,
}

impl PtySession {
    /// Spawn a default shell in the PTY
    pub fn spawn(
        shell: Option<&str>,
        size: PtySize,
        output_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<Self, PtyError> {
        let shell_cmd = shell
            .map(|s| s.to_string())
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| Self::default_shell());

        let mut cmd = CommandBuilder::new(&shell_cmd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "stealthterm");

        // Ensure UTF-8 environment
        if std::env::var("LANG").is_err() {
            cmd.env("LANG", "en_US.UTF-8");
        }

        // Inject OSC 133 shell integration and use login shell
        if shell_cmd.contains("bash") {
            Self::setup_bash_integration(&mut cmd);
        } else if shell_cmd.contains("zsh") {
            Self::setup_zsh_integration(&mut cmd);
        } else if shell_cmd.contains("powershell") || shell_cmd.contains("pwsh") {
            cmd.arg("-NoLogo");
        } else {
            // Unknown shell — just use login flag if possible
            cmd.arg("-l");
        }

        Self::spawn_internal(cmd, size, output_tx)
    }

    /// Setup bash with OSC 133 shell integration
    fn setup_bash_integration(cmd: &mut CommandBuilder) {
        let init_script = r#"
# Source user's normal login config
[ -f /etc/profile ] && . /etc/profile
for f in ~/.bash_profile ~/.bash_login ~/.profile; do
    [ -f "$f" ] && . "$f" && break
done
[ -f ~/.bashrc ] && . ~/.bashrc

# StealthTerm OSC 133 shell integration
__stealthterm_cmd_started=""
__stealthterm_precmd() {
    local e=$?
    if [ -n "$__stealthterm_cmd_started" ]; then
        printf '\e]133;D;%d\a' "$e"
    fi
    __stealthterm_cmd_started=""
    printf '\e]133;A\a'
}
__stealthterm_preexec() {
    if [ -z "$__stealthterm_cmd_started" ]; then
        __stealthterm_cmd_started=1
        printf '\e]133;C\a'
    fi
}
if [[ "$PROMPT_COMMAND" != *"__stealthterm_precmd"* ]]; then
    PROMPT_COMMAND="__stealthterm_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
if [[ "$PS1" != *'133;B'* ]]; then
    PS1="${PS1}\[\e]133;B\a\]"
fi
trap '__stealthterm_preexec' DEBUG
"#;
        // Write init script to temp file
        let init_path = std::env::temp_dir().join("stealthterm_bash_init.sh");
        if let Err(e) = std::fs::write(&init_path, init_script) {
            tracing::warn!("Failed to write bash init script: {}", e);
            cmd.arg("-l");
            return;
        }
        cmd.arg("--rcfile");
        cmd.arg(init_path.to_string_lossy().as_ref());
    }

    /// Setup zsh with OSC 133 shell integration
    fn setup_zsh_integration(cmd: &mut CommandBuilder) {
        let zdotdir = std::env::temp_dir().join("stealthterm_zsh");
        let _ = std::fs::create_dir_all(&zdotdir);

        // Get user's real ZDOTDIR (default to $HOME)
        let real_zdotdir = std::env::var("ZDOTDIR")
            .unwrap_or_else(|_| std::env::var("HOME").unwrap_or_default());

        let zshrc_content = format!(
            r#"
# Restore real ZDOTDIR and source user config
export ZDOTDIR="{real_zdotdir}"
[ -f "$ZDOTDIR/.zshenv" ] && . "$ZDOTDIR/.zshenv"
[ -f "$ZDOTDIR/.zprofile" ] && . "$ZDOTDIR/.zprofile"
[ -f "$ZDOTDIR/.zshrc" ] && . "$ZDOTDIR/.zshrc"

# StealthTerm OSC 133 shell integration
__stealthterm_cmd_started=""
__stealthterm_precmd() {{
    local e=$?
    if [[ -n "$__stealthterm_cmd_started" ]]; then
        print -Pn "\e]133;D;$e\a"
    fi
    __stealthterm_cmd_started=""
    print -Pn "\e]133;A\a"
}}
__stealthterm_preexec() {{
    __stealthterm_cmd_started=1
    print -Pn "\e]133;C\a"
}}
[[ "${{precmd_functions[(r)__stealthterm_precmd]}}" ]] || precmd_functions+=(__stealthterm_precmd)
[[ "${{preexec_functions[(r)__stealthterm_preexec]}}" ]] || preexec_functions+=(__stealthterm_preexec)
if [[ "$PS1" != *'133;B'* ]]; then
    PS1="${{PS1}}%{{$(print -Pn '\e]133;B\a')%}}"
fi
"#
        );

        let zshrc_path = zdotdir.join(".zshrc");
        if let Err(e) = std::fs::write(&zshrc_path, zshrc_content) {
            tracing::warn!("Failed to write zsh init script: {}", e);
            cmd.arg("-l");
            return;
        }

        cmd.env("ZDOTDIR", zdotdir.to_string_lossy().as_ref());
        cmd.arg("-l");
    }

    /// Get default shell for current platform
    fn default_shell() -> String {
        if cfg!(target_os = "windows") {
            // Windows: try to find PowerShell 7 (pwsh) or Windows PowerShell
            Self::find_windows_shell()
        } else {
            // Unix: try to get user shell from /etc/passwd
            Self::unix_user_shell()
                .unwrap_or_else(|| {
                    // Fallback: /bin/bash → /bin/sh
                    if std::path::Path::new("/bin/bash").exists() {
                        "/bin/bash".to_string()
                    } else {
                        "/bin/sh".to_string()
                    }
                })
        }
    }

    /// Windows: find available shell by priority (pwsh → powershell → cmd)
    #[cfg(target_os = "windows")]
    fn find_windows_shell() -> String {
        use std::path::Path;

        // 1. PowerShell 7+ (pwsh.exe) — search in PATH
        if let Ok(path_env) = std::env::var("PATH") {
            for dir in path_env.split(';') {
                let pwsh = format!(r"{}\pwsh.exe", dir);
                if Path::new(&pwsh).exists() {
                    return pwsh;
                }
            }
        }

        // 2. Windows PowerShell 5.x
        if let Ok(sysroot) = std::env::var("SystemRoot") {
            let ps = format!(r"{}\System32\WindowsPowerShell\v1.0\powershell.exe", sysroot);
            if Path::new(&ps).exists() {
                return ps;
            }
        }

        // 3. Fall back to cmd.exe
        if let Ok(comspec) = std::env::var("COMSPEC") {
            if Path::new(&comspec).exists() {
                return comspec;
            }
        }
        "cmd.exe".to_string()
    }

    #[cfg(not(target_os = "windows"))]
    fn find_windows_shell() -> String {
        "/bin/sh".to_string()
    }

    /// Get the current user's login shell from /etc/passwd
    #[cfg(not(target_os = "windows"))]
    fn unix_user_shell() -> Option<String> {
        use std::io::BufRead;
        let uid = unsafe { libc::getuid() };
        let file = std::fs::File::open("/etc/passwd").ok()?;
        for line in std::io::BufReader::new(file).lines() {
            let line = line.ok()?;
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 7 {
                if let Ok(entry_uid) = fields[2].parse::<u32>() {
                    if entry_uid == uid {
                        let shell = fields[6].trim();
                        if !shell.is_empty() && shell != "/usr/sbin/nologin" && shell != "/bin/false" {
                            return Some(shell.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    fn unix_user_shell() -> Option<String> {
        None
    }

    /// Spawn an arbitrary command in the PTY.
    pub fn spawn_command(
        program: &str,
        args: &[&str],
        size: PtySize,
        output_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<Self, PtyError> {
        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        debug!("Spawning command in PTY: {} {:?}", program, args);
        Self::spawn_internal(cmd, size, output_tx)
    }

    /// Internal: open PTY, spawn command, wire reader thread
    fn spawn_internal(
        cmd: CommandBuilder,
        size: PtySize,
        output_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<Self, PtyError> {
        let pty_system = NativePtySystem::default();
        let pty_size = NativePtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(pty_size)
            .map_err(|e| PtyError::Create(e.to_string()))?;

        let child = pair.slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        // Explicitly drop slave end — standard ConPTY practice:
        // parent no longer needs slave, close it to avoid handle leak
        drop(pair.slave);

        let writer = pair.master
            .take_writer()
            .map_err(|e| PtyError::Create(e.to_string()))?;

        let mut reader = pair.master
            .try_clone_reader()
            .map_err(|e| PtyError::Create(e.to_string()))?;

        // Spawn reader thread
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        debug!("PTY reader got EOF");
                        break;
                    }
                    Ok(n) => {
                        if output_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("PTY read error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            size,
            master: Arc::new(std::sync::Mutex::new(pair.master)),
            _child: child,
        })
    }

    pub async fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        let mut w = self.writer.lock().await;
        w.write_all(data)?;
        Ok(())
    }

    pub async fn resize(&mut self, size: PtySize) -> Result<(), PtyError> {
        self.size = size;
        self.master
            .lock()
            .unwrap()
            .resize(NativePtySize {
                rows: size.rows,
                cols: size.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Create(format!("resize failed: {}", e)))?;
        Ok(())
    }
}
