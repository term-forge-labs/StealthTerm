use stealthterm_ssh::config::SshConfig;
use stealthterm_ssh::client::SshClient;
use tokio::sync::mpsc;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct DiskInfo {
    pub mount: String,
    pub used_percent: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ServerStats {
    pub cpu_percent: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disks: Vec<DiskInfo>,
    pub net_rx_rate: f64,
    pub net_tx_rate: f64,
}

pub struct ServerMonitor {
    stats_rx: mpsc::UnboundedReceiver<ServerStats>,
    /// Drop this to kill the SSH command session
    _input_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    current_stats: Option<ServerStats>,
}

impl ServerMonitor {
    /// Start monitoring on a remote server via SSH.
    /// Spawns a background task that runs a stats-gathering loop command.
    pub fn start(config: SshConfig, ctx: egui::Context, runtime: &tokio::runtime::Runtime) -> Self {
        let (stats_tx, stats_rx) = mpsc::unbounded_channel();
        let (output_tx, mut output_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // The monitoring command: outputs stats every 5 seconds in a parseable format.
        // No PTY escape codes since we parse raw text.
        let command = concat!(
            "while true; do ",
            "echo '---STATS---'; ",
            "head -1 /proc/stat; ",
            "free -b 2>/dev/null | grep Mem || vm_stat 2>/dev/null | head -5; ",
            "df -B1 / 2>/dev/null | tail -1; ",
            "cat /proc/net/dev 2>/dev/null | grep -v lo: | grep -v Inter | grep -v face | head -1; ",
            "sleep 5; ",
            "done"
        );

        let input_tx = runtime.block_on(async {
            SshClient::connect_command(config, command, output_tx, 80, 24).await.ok()
        });

        // Background task: accumulate output, parse stats blocks, send parsed results
        runtime.spawn(async move {
            let mut buffer = String::new();
            let mut prev_cpu: Option<(u64, u64)> = None; // (idle, total)
            let mut prev_net: Option<(u64, u64, Instant)> = None; // (rx, tx, time)

            while let Some(chunk) = output_rx.recv().await {
                if let Ok(text) = String::from_utf8(chunk) {
                    buffer.push_str(&text);
                }

                // Process complete blocks
                while let Some(start) = buffer.find("---STATS---") {
                    // Find the next block boundary or end of buffer
                    let after_marker = start + "---STATS---".len();
                    let next_block = buffer[after_marker..].find("---STATS---");
                    let block_end = match next_block {
                        Some(pos) => after_marker + pos,
                        None => {
                            // Check if we have enough lines to parse
                            let remaining = &buffer[after_marker..];
                            let line_count = remaining.lines().filter(|l| !l.trim().is_empty()).count();
                            if line_count < 3 {
                                break; // Wait for more data
                            }
                            buffer.len()
                        }
                    };

                    let block = buffer[after_marker..block_end].to_string();
                    buffer = buffer[block_end..].to_string();

                    if let Some(stats) = parse_stats_block(&block, &mut prev_cpu, &mut prev_net) {
                        let _ = stats_tx.send(stats);
                        ctx.request_repaint();
                    }
                }

                // Prevent buffer from growing unbounded
                if buffer.len() > 8192 {
                    buffer = buffer[buffer.len() - 4096..].to_string();
                }
            }
        });

        Self {
            stats_rx,
            _input_tx: input_tx,
            current_stats: None,
        }
    }

    /// Poll for new stats (non-blocking). Call each frame.
    pub fn poll(&mut self) {
        while let Ok(stats) = self.stats_rx.try_recv() {
            self.current_stats = Some(stats);
        }
    }

    /// Get the latest stats, if available.
    pub fn stats(&self) -> Option<&ServerStats> {
        self.current_stats.as_ref()
    }
}

fn parse_stats_block(
    block: &str,
    prev_cpu: &mut Option<(u64, u64)>,
    prev_net: &mut Option<(u64, u64, Instant)>,
) -> Option<ServerStats> {
    let lines: Vec<&str> = block.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    let mut stats = ServerStats::default();

    for line in &lines {
        if line.starts_with("cpu ") {
            // /proc/stat: cpu user nice system idle iowait irq softirq steal
            let vals: Vec<u64> = line.split_whitespace()
                .skip(1)
                .filter_map(|v| v.parse().ok())
                .collect();
            if vals.len() >= 4 {
                let idle = vals[3];
                let total: u64 = vals.iter().sum();
                if let Some((prev_idle, prev_total)) = prev_cpu {
                    let d_total = total.saturating_sub(*prev_total);
                    let d_idle = idle.saturating_sub(*prev_idle);
                    if d_total > 0 {
                        stats.cpu_percent = 100.0 * (1.0 - d_idle as f32 / d_total as f32);
                    }
                }
                *prev_cpu = Some((idle, total));
            }
        } else if line.starts_with("Mem:") {
            // free -b: Mem: total used free shared buff/cache available
            let vals: Vec<u64> = line.split_whitespace()
                .skip(1)
                .filter_map(|v| v.parse().ok())
                .collect();
            if vals.len() >= 2 {
                stats.mem_total = vals[0];
                stats.mem_used = vals[1];
            }
        } else if line.starts_with('/') {
            // df -B1 output: filesystem 1B-blocks used available use% mountpoint
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                // Try to find the mount point (last field) and use% (second to last)
                let mount = parts[parts.len() - 1].to_string();
                let use_pct_str = parts[parts.len() - 2];
                if let Some(pct) = use_pct_str.strip_suffix('%') {
                    if let Ok(pct_val) = pct.parse::<f32>() {
                        stats.disks.push(DiskInfo {
                            mount,
                            used_percent: pct_val,
                        });
                    }
                }
            }
        } else if line.contains(':') && !line.starts_with("cpu") && !line.starts_with("Mem") {
            // /proc/net/dev: iface: rx_bytes rx_packets ... tx_bytes tx_packets ...
            let after_colon = line.split(':').nth(1);
            if let Some(data) = after_colon {
                let vals: Vec<u64> = data.split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect();
                if vals.len() >= 9 {
                    let rx_bytes = vals[0];
                    let tx_bytes = vals[8];
                    let now = Instant::now();
                    if let Some((prev_rx, prev_tx, prev_time)) = prev_net {
                        let dt = now.duration_since(*prev_time).as_secs_f64();
                        if dt > 0.0 {
                            stats.net_rx_rate = rx_bytes.saturating_sub(*prev_rx) as f64 / dt;
                            stats.net_tx_rate = tx_bytes.saturating_sub(*prev_tx) as f64 / dt;
                        }
                    }
                    *prev_net = Some((rx_bytes, tx_bytes, now));
                }
            }
        }
    }

    Some(stats)
}

/// Format bytes per second into human-readable rate
pub fn format_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 {
        format!("{:.0}B/s", bytes_per_sec)
    } else if bytes_per_sec < 1024.0 * 1024.0 {
        format!("{:.1}K/s", bytes_per_sec / 1024.0)
    } else if bytes_per_sec < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1}M/s", bytes_per_sec / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G/s", bytes_per_sec / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Format bytes into human-readable size
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
