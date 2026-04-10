use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0); // 0=En, 1=Zh

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lang {
    En,
    Zh,
}

impl Lang {
    pub fn code(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Zh => "zh",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "zh" => Lang::Zh,
            _ => Lang::En,
        }
    }
}

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::Zh
    } else {
        Lang::En
    }
}

/// Translate a key to the current language
pub fn t(key: &str) -> &'static str {
    match lang() {
        Lang::En => en(key),
        Lang::Zh => zh(key),
    }
}

/// Translate with format support — returns String for keys needing interpolation
pub fn tf(key: &str, args: &[&str]) -> String {
    let template = t(key);
    let mut result = template.to_string();
    for arg in args {
        result = result.replacen("{}", arg, 1);
    }
    result
}

fn en(key: &str) -> &'static str {
    match key {
        // ===== Lock screen =====
        "lock.enter_password" => "Enter access password",
        "lock.password_hint" => "Password",
        "lock.unlock" => "Unlock",
        "lock.wrong_password" => "Wrong password",

        // ===== Title bar / toolbar =====
        "toolbar.settings" => "Settings",
        "toolbar.open_local_terminal" => "Open local terminal",
        "toolbar.split_screen" => "Split screen",
        "toolbar.close_split" => "Close split",
        "toolbar.batch_mode" => "Batch execute",
        "toolbar.close_batch" => "Close batch mode",
        "toolbar.batch_active" => "Batch",

        // ===== Split menu =====
        "split.horizontal" => "⬅➡ Split horizontal",
        "split.vertical" => "⬆⬇ Split vertical",

        // ===== Main menu =====
        "menu.about" => "About",

        // ===== About dialog =====
        "about.title" => "About StealthTerm",
        "about.version" => "Version: 0.1.0-alpha1",
        "about.description" => "Modern Terminal",
        "about.ok" => "OK",

        // ===== Close confirm =====
        "close.title" => "Confirm Exit",
        "close.ssh_running" => "SSH sessions currently running",
        "close.warning" => "Closing will interrupt all unfinished tasks",
        "close.ok" => "OK",
        "close.cancel" => "Cancel",

        // ===== No terminal =====
        "app.no_terminal" => "No terminal open. Double-click an SSH connection in the left sidebar to open a new tab.",

        // ===== Batch select =====
        "batch.title" => "📡 Batch Execute - Select Terminals",
        "batch.select_hint" => "Select terminal tabs for synchronized command execution:",
        "batch.select_all" => "✅ Select All",
        "batch.deselect_all" => "❎ Deselect All",
        "batch.confirm" => "✅ Confirm",
        "batch.cancel" => "❌ Cancel",

        // ===== Settings panel =====
        "settings.title" => "⚙ Settings",
        "settings.appearance" => "🎨 Appearance",
        "settings.terminal" => "💻 Terminal",
        "settings.security" => "🔒 Security",
        "settings.language" => "🌐 Language",
        "settings.about" => "ℹ About",
        "settings.theme" => "Terminal theme:",
        "settings.font_size" => "Font size:",
        "settings.show_line_numbers" => "Show line numbers",
        "settings.cursor_blink" => "Cursor blink",
        "settings.scrollback" => "Scrollback buffer: {} lines",
        "settings.clear_history" => "🗑 Clear command history",
        "settings.clear_history_confirm" => "Clear all command history?",
        "settings.clear_history_done" => "✅ Command history cleared",
        "settings.confirm" => "Confirm",
        "settings.cancel" => "Cancel",
        "settings.password_set" => "✅ Access password set",
        "settings.current_password" => "Current password:",
        "settings.new_password" => "New password:",
        "settings.confirm_password" => "Confirm password:",
        "settings.change_password" => "Change password",
        "settings.enter_current" => "Please enter current password",
        "settings.wrong_current" => "Current password is incorrect",
        "settings.mismatch" => "Passwords do not match",
        "settings.too_short" => "Password must be at least 4 characters",
        "settings.password_changed" => "✅ Password changed",
        "settings.set_password" => "Set access password",
        "settings.remove_password" => "Remove password",
        "settings.enter_password" => "Please enter a password",
        "settings.password_set_ok" => "✅ Access password set",
        "settings.password_removed" => "✅ Access password removed",
        "settings.auto_lock" => "Auto-lock after idle:",
        "settings.no_auto_lock" => "Disabled",
        "settings.minutes" => "{} minutes",
        "settings.version_info" => "StealthTerm v0.1.0-alpha1",
        "settings.modern_terminal" => "Modern Terminal",
        "settings.save" => "💾 Save Settings",
        "settings.language_label" => "Language:",

        // ===== Sidebar =====
        "sidebar.file_manager" => "Files",
        "sidebar.ssh_connections" => "SSH",
        "sidebar.search_placeholder" => "🔍 Search...",
        "sidebar.loading_working_dir" => "Loading working directory...",
        "sidebar.new_ssh" => "New SSH Connection",
        "sidebar.open" => "Open",
        "sidebar.edit" => "Edit",
        "sidebar.copy" => "Copy",
        "sidebar.delete" => "Delete",
        "sidebar.paste" => "Paste Connection",

        // ===== Tab bar context menu =====
        "tab.duplicate_ssh" => "Duplicate SSH Connection",
        "tab.close" => "Close Tab",
        "tab.close_others" => "Close Other Tabs",
        "tab.close_left" => "Close Tabs to the Left",
        "tab.close_right" => "Close Tabs to the Right",
        "tab.close_all" => "Close All Tabs",
        "tab.disconnected" => "Disconnected",

        // ===== Status bar =====
        "status.connected" => "Connected",
        "status.disconnected" => "Disconnected",

        // ===== Connection panel =====
        "conn.title" => "SSH Connection",
        "conn.name" => "Name",
        "conn.host" => "Host",
        "conn.port" => "Port",
        "conn.username" => "Username",
        "conn.auth_method" => "Auth Method",
        "conn.password" => "Password",
        "conn.auth_password" => "Password",
        "conn.auth_pubkey" => "Private Key",
        "conn.pubkey_hint" => "The corresponding public key must be added to the server's ~/.ssh/authorized_keys",
        "conn.key_file" => "Private Key File",
        "conn.key_path_hint" => "e.g. ~/.ssh/id_ed25519",
        "conn.browse" => "Browse…",
        "conn.passphrase" => "Key Passphrase",
        "conn.passphrase_hint" => "Leave empty if not protected",
        "conn.group" => "Group",
        "conn.save" => "Save",
        "conn.cancel" => "Cancel",
        "conn.fill_password" => "Fill from saved password",
        "conn.select_password" => "Select a saved password:",

        // ===== SFTP panel =====
        "sftp.title" => "SFTP File Manager",
        "sftp.local" => "Local",
        "sftp.remote" => "Remote",
        "sftp.not_connected" => "Not connected.\nOpen an SSH session to browse remote files.",
        "sftp.transfer_queue" => "Transfer Queue",
        "sftp.no_transfers" => "No active transfers.",
        "sftp.download" => "Download",
        "sftp.upload" => "Upload",
        "sftp.status_pending" => "Pending",
        "sftp.status_transferring" => "Transferring",
        "sftp.status_paused" => "Paused",
        "sftp.status_done" => "Done",
        "sftp.status_failed" => "Failed",

        // ===== Command palette =====
        "cmd.new_local_terminal" => "New Local Terminal",
        "cmd.new_ssh_connection" => "New SSH Connection",
        "cmd.close_tab" => "Close Tab",
        "cmd.next_tab" => "Next Tab",
        "cmd.prev_tab" => "Previous Tab",
        "cmd.toggle_sidebar" => "Toggle Sidebar",
        "cmd.search_terminal" => "Search Terminal",
        "cmd.toggle_sftp" => "Toggle SFTP Panel",
        "cmd.toggle_batch" => "Toggle Batch Mode",
        "cmd.split_horizontal" => "Split Horizontal",
        "cmd.split_vertical" => "Split Vertical",
        "cmd.font_increase" => "Increase Font Size",
        "cmd.font_decrease" => "Decrease Font Size",
        "cmd.font_reset" => "Reset Font Size",
        "cmd.toggle_fullscreen" => "Toggle Fullscreen",
        "cmd.open_settings" => "Open Settings",
        "cmd.theme_prefix" => "Theme:",
        "cmd.search_placeholder" => "Type a command...",
        "cmd.no_results" => "No matching commands",

        // ===== Search bar =====
        "search.placeholder" => "Search... (Regex supported)",
        "search.no_results" => "No results",

        // ===== Paste confirm =====
        "paste.title" => "Confirm Paste",
        "paste.line_count" => "lines",
        "paste.confirm" => "Paste",
        "paste.cancel" => "Cancel",
        "paste.dont_ask" => "Don't ask again",

        // ===== Snippet picker =====
        "snippet.title" => "📋 Command Snippets",
        "snippet.search" => "Search snippets...",
        "snippet.no_results" => "No matching snippets",
        "snippet.insert" => "Insert",

        // ===== Server monitor =====
        "monitor.cpu" => "CPU",
        "monitor.mem" => "Mem",
        "monitor.net_down" => "↓",
        "monitor.net_up" => "↑",

        // ===== Transfer queue =====
        "transfer.title" => "Transfer Queue",
        "transfer.no_items" => "No transfers",
        "transfer.cancel" => "Cancel",
        "transfer.retry" => "Retry",

        // ===== SSH errors =====
        "ssh.error_pubkey_rejected" => "Public key authentication rejected by server\r\n",
        "ssh.error_pubkey_auth" => "Public key authentication error",
        "ssh.error_load_key" => "Failed to load private key",
        "ssh.error_auth_failed" => "SSH authentication failed\r\n",

        // ===== Terminal =====
        "terminal.title" => "Terminal",
        "terminal.local" => "Local Terminal",

        // ===== Terminal panel =====
        "terminal.upload_success" => "File uploaded successfully",
        "terminal.save_file" => "Save File",
        "terminal.upload_success_title" => "Upload Success",
        "terminal.ok" => "OK",
        "terminal.paste_confirm_title" => "Confirm Paste",
        "terminal.paste_confirm_msg" => "Multi-line text detected ({} lines), confirm paste?",
        "terminal.paste_confirm" => "Confirm Paste",
        "terminal.paste_cancel" => "Cancel",
        "terminal.ctx_select_all" => "Select All",
        "terminal.ctx_clear" => "Clear Screen",
        "terminal.ctx_jump_prev_cmd" => "Jump to Previous Command",
        "terminal.ctx_expand" => "Expand Output",
        "terminal.ctx_collapse" => "Collapse Output",
        "terminal.ctx_copy_output" => "Copy Command Output",
        "terminal.ctx_collapse_all" => "Collapse All",
        "terminal.ctx_expand_all" => "Expand All",
        "terminal.convert_tabs" => "Convert Tab to 4 spaces",
        "terminal.remove_crlf" => "Remove Windows line endings (\\r\\n)",
        "terminal.paste_preview" => "Preview after conversion:",
        "terminal.paste_enter_hint" => "(Enter to confirm, Esc to cancel)",
        "terminal.ctx_copy" => "Copy",
        "terminal.ctx_paste" => "Paste (Ctrl+Shift+V)",

        // ===== Sidebar file manager =====
        "sidebar.remote_files" => "🌍 Remote Files",
        "sidebar.local_files" => "💻 Local Files",
        "sidebar.download" => "Download",
        "sidebar.upload" => "Upload",

        // ===== Snippet categories =====
        "snippet.category_all" => "All",
        "snippet.category_review" => "Code Review",
        "snippet.category_bugfix" => "Bug Fix",
        "snippet.category_feature" => "Feature Dev",
        "snippet.category_docs" => "Documentation",
        "snippet.category_label" => "Category:",

        // ===== Transfer queue panel =====
        "transfer.speed" => "Speed",
        "transfer.remaining" => "Remaining",
        "transfer.pause" => "Pause",
        "transfer.resume" => "Resume",

        // ===== Role dashboard =====
        "role.backend" => "Backend Dev",
        "role.frontend" => "Frontend Dev",
        "role.qa" => "QA Engineer",
        "role.product_manager" => "Product Manager",
        "role.documentation" => "Documentation",
        "role.security" => "Security Audit",
        "role.not_started" => "Not Started",
        "role.dashboard_title" => "Role Management (Boss Mode)",
        "role.list_heading" => "Role List",
        "role.view" => "View",

        // ===== Zmodem =====
        "zmodem.windows_hint" => "Windows: Download https://github.com/trzsz/lrzsz-win32/releases and add to PATH",
        "zmodem.linux_hint" => "Linux: sudo apt install lrzsz or sudo yum install lrzsz",

        // ===== Misc =====
        "misc.copy_suffix" => "-copy",

        _ => "???",
    }
}

fn zh(key: &str) -> &'static str {
    match key {
        // ===== Lock screen =====
        "lock.enter_password" => "请输入访问口令",
        "lock.password_hint" => "口令",
        "lock.unlock" => "解锁",
        "lock.wrong_password" => "口令错误",

        // ===== Title bar / toolbar =====
        "toolbar.settings" => "设置",
        "toolbar.open_local_terminal" => "打开本地终端",
        "toolbar.split_screen" => "分屏",
        "toolbar.close_split" => "关闭分屏",
        "toolbar.batch_mode" => "批量执行",
        "toolbar.close_batch" => "关闭批量执行",
        "toolbar.batch_active" => "批量中",

        // ===== Split menu =====
        "split.horizontal" => "⬅➡ 左右分屏",
        "split.vertical" => "⬆⬇ 上下分屏",

        // ===== Main menu =====
        "menu.about" => "关于",

        // ===== About dialog =====
        "about.title" => "关于 StealthTerm",
        "about.version" => "版本: 0.1.0-alpha1",
        "about.description" => "现代化终端",
        "about.ok" => "确定",

        // ===== Close confirm =====
        "close.title" => "确认退出",
        "close.ssh_running" => "个 SSH 会话正在运行",
        "close.warning" => "关闭程序将中断所有未完成的任务",
        "close.ok" => "确定",
        "close.cancel" => "取消",

        // ===== No terminal =====
        "app.no_terminal" => "没有打开的终端，请双击左侧ssh连接列表中的条目打开新标签页。",

        // ===== Batch select =====
        "batch.title" => "📡 批量执行 - 选择终端",
        "batch.select_hint" => "选择要同步执行命令的终端标签：",
        "batch.select_all" => "✅ 全选",
        "batch.deselect_all" => "❎ 取消全选",
        "batch.confirm" => "✅ 确认",
        "batch.cancel" => "❌ 取消",

        // ===== Settings panel =====
        "settings.title" => "⚙ 设置",
        "settings.appearance" => "🎨 外观",
        "settings.terminal" => "💻 终端",
        "settings.security" => "🔒 安全",
        "settings.language" => "🌐 语言",
        "settings.about" => "ℹ 关于",
        "settings.theme" => "终端主题：",
        "settings.font_size" => "字体大小：",
        "settings.show_line_numbers" => "显示行号",
        "settings.cursor_blink" => "光标闪烁",
        "settings.scrollback" => "滚动缓冲区：{} 行",
        "settings.clear_history" => "🗑 清除历史命令",
        "settings.clear_history_confirm" => "确认清除所有历史命令？",
        "settings.clear_history_done" => "✅ 历史命令已清除",
        "settings.confirm" => "确认",
        "settings.cancel" => "取消",
        "settings.password_set" => "✅ 已设置访问口令",
        "settings.current_password" => "当前口令：",
        "settings.new_password" => "新口令：　",
        "settings.confirm_password" => "确认口令：",
        "settings.change_password" => "修改口令",
        "settings.enter_current" => "请输入当前口令",
        "settings.wrong_current" => "当前口令不正确",
        "settings.mismatch" => "两次输入不一致",
        "settings.too_short" => "口令至少4位",
        "settings.password_changed" => "✅ 口令已修改",
        "settings.set_password" => "设置访问口令",
        "settings.remove_password" => "清除口令",
        "settings.enter_password" => "请输入口令",
        "settings.password_set_ok" => "✅ 已设置访问口令",
        "settings.password_removed" => "✅ 已清除访问口令",
        "settings.auto_lock" => "空闲自动锁定：",
        "settings.no_auto_lock" => "不自动锁定",
        "settings.minutes" => "{} 分钟",
        "settings.version_info" => "StealthTerm v0.1.0-alpha1",
        "settings.modern_terminal" => "现代化终端",
        "settings.save" => "💾 保存设置",
        "settings.language_label" => "语言：",

        // ===== Sidebar =====
        "sidebar.file_manager" => "文件管理",
        "sidebar.ssh_connections" => "ssh连接",
        "sidebar.search_placeholder" => "🔍 搜索...",
        "sidebar.loading_working_dir" => "正在获取工作目录...",
        "sidebar.new_ssh" => "新建 SSH 连接",
        "sidebar.open" => "打开",
        "sidebar.edit" => "编辑",
        "sidebar.copy" => "复制",
        "sidebar.delete" => "删除",
        "sidebar.paste" => "粘贴连接",

        // ===== Tab bar context menu =====
        "tab.duplicate_ssh" => "复制SSH连接",
        "tab.close" => "关闭当前标签",
        "tab.close_others" => "关闭所有非活动标签",
        "tab.close_left" => "关闭左侧所有标签",
        "tab.close_right" => "关闭右侧所有标签",
        "tab.close_all" => "关闭全部标签",
        "tab.disconnected" => "已断开",

        // ===== Status bar =====
        "status.connected" => "已连接",
        "status.disconnected" => "未连接",

        // ===== Connection panel =====
        "conn.title" => "SSH 连接",
        "conn.name" => "名称",
        "conn.host" => "主机",
        "conn.port" => "端口",
        "conn.username" => "用户名",
        "conn.auth_method" => "认证方式",
        "conn.password" => "密码",
        "conn.auth_password" => "密码",
        "conn.auth_pubkey" => "私钥",
        "conn.pubkey_hint" => "对应的公钥需要添加到服务器的 ~/.ssh/authorized_keys",
        "conn.key_file" => "私钥文件",
        "conn.key_path_hint" => "如 ~/.ssh/id_ed25519",
        "conn.browse" => "浏览…",
        "conn.passphrase" => "密钥密码",
        "conn.passphrase_hint" => "无密码保护则留空",
        "conn.group" => "分组",
        "conn.save" => "保存",
        "conn.cancel" => "取消",
        "conn.fill_password" => "使用已保存的密码填充",
        "conn.select_password" => "选择已保存的密码：",

        // ===== SFTP panel =====
        "sftp.title" => "SFTP 文件管理器",
        "sftp.local" => "本地",
        "sftp.remote" => "远程",
        "sftp.not_connected" => "未连接。\n请先打开 SSH 会话以浏览远程文件。",
        "sftp.transfer_queue" => "传输队列",
        "sftp.no_transfers" => "没有活动的传输。",
        "sftp.download" => "下载",
        "sftp.upload" => "上传",
        "sftp.status_pending" => "等待中",
        "sftp.status_transferring" => "传输中",
        "sftp.status_paused" => "已暂停",
        "sftp.status_done" => "完成",
        "sftp.status_failed" => "失败",

        // ===== Command palette =====
        "cmd.new_local_terminal" => "新建本地终端",
        "cmd.new_ssh_connection" => "新建 SSH 连接",
        "cmd.close_tab" => "关闭标签",
        "cmd.next_tab" => "下一个标签",
        "cmd.prev_tab" => "上一个标签",
        "cmd.toggle_sidebar" => "切换侧边栏",
        "cmd.search_terminal" => "搜索终端",
        "cmd.toggle_sftp" => "切换 SFTP 面板",
        "cmd.toggle_batch" => "切换批量执行",
        "cmd.split_horizontal" => "左右分屏",
        "cmd.split_vertical" => "上下分屏",
        "cmd.font_increase" => "增大字体",
        "cmd.font_decrease" => "减小字体",
        "cmd.font_reset" => "重置字体大小",
        "cmd.toggle_fullscreen" => "切换全屏",
        "cmd.open_settings" => "打开设置",
        "cmd.theme_prefix" => "主题：",
        "cmd.search_placeholder" => "输入命令...",
        "cmd.no_results" => "没有匹配的命令",

        // ===== Search bar =====
        "search.placeholder" => "搜索...（支持正则表达式）",
        "search.no_results" => "无结果",

        // ===== Paste confirm =====
        "paste.title" => "确认粘贴",
        "paste.line_count" => "行",
        "paste.confirm" => "粘贴",
        "paste.cancel" => "取消",
        "paste.dont_ask" => "不再提示",

        // ===== Snippet picker =====
        "snippet.title" => "📋 命令片段",
        "snippet.search" => "搜索片段...",
        "snippet.no_results" => "没有匹配的片段",
        "snippet.insert" => "插入",

        // ===== Server monitor =====
        "monitor.cpu" => "CPU",
        "monitor.mem" => "内存",
        "monitor.net_down" => "↓",
        "monitor.net_up" => "↑",

        // ===== Transfer queue =====
        "transfer.title" => "传输队列",
        "transfer.no_items" => "没有传输任务",
        "transfer.cancel" => "取消",
        "transfer.retry" => "重试",

        // ===== SSH errors =====
        "ssh.error_pubkey_rejected" => "服务器拒绝了公钥认证\r\n",
        "ssh.error_pubkey_auth" => "公钥认证错误",
        "ssh.error_load_key" => "加载私钥失败",
        "ssh.error_auth_failed" => "SSH 认证失败\r\n",

        // ===== Terminal =====
        "terminal.title" => "终端",
        "terminal.local" => "本地终端",

        // ===== Terminal panel =====
        "terminal.upload_success" => "文件上传成功",
        "terminal.save_file" => "保存文件",
        "terminal.upload_success_title" => "上传成功",
        "terminal.ok" => "确定",
        "terminal.paste_confirm_title" => "确认粘贴",
        "terminal.paste_confirm_msg" => "检测到多行文本（共 {} 行），确认粘贴？",
        "terminal.paste_confirm" => "确认粘贴",
        "terminal.paste_cancel" => "取消",
        "terminal.ctx_select_all" => "全选",
        "terminal.ctx_clear" => "清屏",
        "terminal.ctx_jump_prev_cmd" => "跳转至上一条命令位置",
        "terminal.ctx_expand" => "展开输出",
        "terminal.ctx_collapse" => "折叠输出",
        "terminal.ctx_copy_output" => "复制命令输出",
        "terminal.ctx_collapse_all" => "全部折叠",
        "terminal.ctx_expand_all" => "全部展开",
        "terminal.convert_tabs" => "转换Tab为4个空格",
        "terminal.remove_crlf" => "消除Windows换行符(\\r\\n)",
        "terminal.paste_preview" => "转换后预览：",
        "terminal.paste_enter_hint" => "（回车确认，Esc取消）",
        "terminal.ctx_copy" => "复制",
        "terminal.ctx_paste" => "粘贴 (Ctrl+Shift+V)",

        // ===== Sidebar file manager =====
        "sidebar.remote_files" => "🌍 远程文件",
        "sidebar.local_files" => "💻 本地文件",
        "sidebar.download" => "下载",
        "sidebar.upload" => "上传",

        // ===== Snippet categories =====
        "snippet.category_all" => "全部",
        "snippet.category_review" => "代码审查",
        "snippet.category_bugfix" => "Bug修复",
        "snippet.category_feature" => "功能开发",
        "snippet.category_docs" => "文档编写",
        "snippet.category_label" => "分类：",

        // ===== Transfer queue panel =====
        "transfer.speed" => "速度",
        "transfer.remaining" => "剩余",
        "transfer.pause" => "暂停",
        "transfer.resume" => "继续",

        // ===== Role dashboard =====
        "role.backend" => "后端开发",
        "role.frontend" => "前端开发",
        "role.qa" => "测试工程师",
        "role.product_manager" => "产品经理",
        "role.documentation" => "文档编写",
        "role.security" => "安全审计",
        "role.not_started" => "未启动",
        "role.dashboard_title" => "多角色管理 (Boss 模式)",
        "role.list_heading" => "角色列表",
        "role.view" => "查看",

        // ===== Zmodem =====
        "zmodem.windows_hint" => "Windows: download https://github.com/trzsz/lrzsz-win32/releases and add to PATH",
        "zmodem.linux_hint" => "Linux: sudo apt install lrzsz 或 sudo yum install lrzsz",

        // ===== Misc =====
        "misc.copy_suffix" => "-副本",

        _ => en(key),
    }
}
