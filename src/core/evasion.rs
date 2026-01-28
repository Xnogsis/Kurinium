#![allow(dead_code)]

use wraith::Peb;
use wraith::manipulation::antidebug;
use wraith::manipulation::hooks::scan_for_hooks;
use wraith::navigation::collect_modules;

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

// Init
// ================================
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
    
    result
}

// Anti-Debug Functions
// ================================
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

// Hook Detection
// ================================
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

// Module Query
// ================================
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
