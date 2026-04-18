/// Configuration loading and resolution for screenpipe-manager.
///
/// Config is read from `config.toml` located in the same directory as the
/// running binary.  If no file is found, built-in defaults are used so the
/// tool is usable out of the box without any setup.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// All user-configurable options for screenpipe-manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// How many days of data to keep. Records older than this are deleted on cleanup.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,

    /// Maximum allowed total disk usage in GB. Informational for now; future
    /// versions may trigger an automatic cleanup when this threshold is exceeded.
    #[serde(default = "default_max_storage_gb")]
    pub max_storage_gb: f64,

    /// Path to the screenpipe data directory.  Empty string means "use the
    /// platform default" (~/.screenpipe on Unix, %USERPROFILE%\.screenpipe on Windows).
    #[serde(default)]
    pub data_dir: String,

    /// Whether screenpipe should record audio.
    #[serde(default = "default_true")]
    pub record_audio: bool,

    /// Whether screenpipe should capture screen frames.
    #[serde(default = "default_true")]
    pub record_screen: bool,

    /// Whether screenpipe should store audio transcriptions.
    #[serde(default = "default_true")]
    pub record_transcription: bool,

    /// App names that should never be recorded.  cleanup will also purge any
    /// existing records for these apps.
    #[serde(default)]
    pub blacklist: Vec<String>,

    /// If non-empty, only these app names are recorded (whitelist mode).
    #[serde(default)]
    pub whitelist: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            retention_days: default_retention_days(),
            max_storage_gb: default_max_storage_gb(),
            data_dir: String::new(),
            record_audio: true,
            record_screen: true,
            record_transcription: true,
            blacklist: Vec::new(),
            whitelist: Vec::new(),
        }
    }
}

impl Config {
    /// Returns the resolved data directory path.
    ///
    /// If `data_dir` is empty, falls back to `~/.screenpipe` (cross-platform).
    pub fn resolved_data_dir(&self) -> PathBuf {
        if !self.data_dir.is_empty() {
            return PathBuf::from(&self.data_dir);
        }
        default_screenpipe_dir()
    }
}

/// Returns the platform-default screenpipe data directory (~/.screenpipe).
pub fn default_screenpipe_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".screenpipe")
}

/// Load configuration from `config.toml` next to the binary.
///
/// Falls back to `Config::default()` if no file exists, so the tool works
/// without any configuration.
pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = config_file_path();

    if !config_path.exists() {
        // No config file — use defaults and proceed silently.
        return Ok(Config::default());
    }

    let raw = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Cannot read {}: {e}", config_path.display()))?;

    let cfg: Config = toml::from_str(&raw)
        .map_err(|e| format!("Invalid TOML in {}: {e}", config_path.display()))?;

    Ok(cfg)
}

/// Path to the config file: same directory as the binary, named `config.toml`.
fn config_file_path() -> PathBuf {
    // std::env::current_exe() can fail in unusual environments; fall back to CWD.
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.toml")))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

// --- serde default helpers ---

fn default_retention_days() -> u32 {
    7
}

fn default_max_storage_gb() -> f64 {
    10.0
}

fn default_true() -> bool {
    true
}
