use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub keep_in_background: bool,
    pub show_tray_icon: bool,
    pub auto_boost_enabled: bool,
    pub ram_threshold: f64,
    #[serde(default = "default_temp_threshold_mb")]
    pub temp_threshold_mb: f64,
    pub cooldown_minutes: u64,
    pub desktop_notifications: bool,
    #[serde(default = "default_true")]
    pub hardware_acceleration: bool,
    #[serde(default = "default_auto")]
    pub graphics_renderer: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    #[serde(default)]
    pub disabled_tabs: Vec<String>,
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_endpoint")]
    pub ai_endpoint: String,
    #[serde(default)]
    pub ai_api_key: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
}

fn default_true() -> bool { true }
fn default_temp_threshold_mb() -> f64 { 1024.0 }
fn default_auto() -> String { "auto".to_string() }
fn default_accent() -> String { "blue".to_string() }
fn default_ai_provider() -> String { "openai".to_string() }
fn default_ai_endpoint() -> String { "https://api.openai.com/v1".to_string() }
fn default_ai_model() -> String { "gpt-4o-mini".to_string() }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            keep_in_background: true,
            show_tray_icon: true,
            auto_boost_enabled: true,
            ram_threshold: 80.0,
            temp_threshold_mb: 1024.0,
            cooldown_minutes: 15,
            desktop_notifications: true,
            hardware_acceleration: true,
            graphics_renderer: "auto".to_string(),
            accent_color: "blue".to_string(),
            disabled_tabs: Vec::new(),
            ai_provider: "openai".to_string(),
            ai_endpoint: "https://api.openai.com/v1".to_string(),
            ai_api_key: String::new(),
            ai_model: "gpt-4o-mini".to_string(),
        }
    }
}

static SETTINGS_INSTANCE: OnceLock<Mutex<AppSettings>> = OnceLock::new();

impl AppSettings {
    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let modern = PathBuf::from(&home).join(".config/io.github.antux912012.Mithrandir/settings.json");
        if modern.exists() {
            return modern;
        }
        let legacy = PathBuf::from(&home).join(".config/io.github.antonio.Mithrandir/settings.json");
        if legacy.exists() {
            return legacy;
        }
        modern
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<AppSettings>(&content) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, content);
        }
    }

    fn instance() -> &'static Mutex<AppSettings> {
        SETTINGS_INSTANCE.get_or_init(|| Mutex::new(Self::load()))
    }

    pub fn get() -> Self {
        Self::instance().lock().unwrap().clone()
    }

    pub fn update<F: FnOnce(&mut AppSettings)>(f: F) {
        let mut lock = Self::instance().lock().unwrap();
        f(&mut lock);
        lock.save();
    }
}
