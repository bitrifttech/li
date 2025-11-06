use anyhow::{Context, Result, anyhow};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub(crate) const DEFAULT_MAX_TOKENS: u32 = 2048;
const DEFAULT_CLASSIFIER_MODEL: &str = "nvidia/nemotron-nano-12b-v2-vl:free";
const DEFAULT_PLANNER_MODEL: &str = "minimax/minimax-m2:free";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub api_key: String,
    pub timeout_secs: u64,
    pub max_tokens: u32,
    pub classifier_model: String,
    pub planner_model: String,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    openrouter_api_key: Option<String>,
    timeout_secs: Option<u64>,
    max_tokens: Option<u32>,
    classifier_model: Option<String>,
    planner_model: Option<String>,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let mut path = home_dir().context("Could not determine home directory")?;
        path.push(".li/config");
        Ok(path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let file_cfg = Self::read_file_config(&path)?;
        let FileConfig {
            openrouter_api_key: file_openrouter_key,
            timeout_secs: file_timeout,
            max_tokens: file_max_tokens,
            classifier_model: file_classifier,
            planner_model: file_planner,
        } = file_cfg;

        // Get OpenRouter API key
        let api_key = Self::env_string("OPENROUTER_API_KEY")?
            .or(file_openrouter_key)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                anyhow!(
                    "OpenRouter API key not found. Set OPENROUTER_API_KEY or add it to {}",
                    path.display()
                )
            })?;

        let timeout_secs = Self::env_u64("LI_TIMEOUT_SECS")?
            .or(file_timeout)
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let max_tokens = Self::env_u32("LI_MAX_TOKENS")?
            .or(file_max_tokens)
            .unwrap_or(DEFAULT_MAX_TOKENS);

        let classifier_model = Self::env_string("LI_CLASSIFIER_MODEL")?
            .or(file_classifier)
            .unwrap_or_else(|| DEFAULT_CLASSIFIER_MODEL.to_string());

        let planner_model = Self::env_string("LI_PLANNER_MODEL")?
            .or(file_planner)
            .unwrap_or_else(|| DEFAULT_PLANNER_MODEL.to_string());

        Ok(Self {
            api_key,
            timeout_secs,
            max_tokens,
            classifier_model,
            planner_model,
        })
    }

    fn read_file_config(path: &Path) -> Result<FileConfig> {
        if !path.exists() {
            return Ok(FileConfig::default());
        }

        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed reading config at {}", path.display()))?;
        let file = serde_json::from_str(&contents)
            .with_context(|| format!("Failed parsing JSON config at {}", path.display()))?;
        Ok(file)
    }

    fn env_string(key: &str) -> Result<Option<String>> {
        match env::var(key) {
            Ok(val) => Ok(Some(val)),
            Err(env::VarError::NotPresent) => Ok(None),
            Err(env::VarError::NotUnicode(_)) => Err(anyhow!("{key} contains invalid UTF-8")),
        }
    }

    fn env_u64(key: &str) -> Result<Option<u64>> {
        if let Some(value) = Self::env_string(key)? {
            let parsed = value
                .parse::<u64>()
                .with_context(|| format!("Failed to parse {key} as u64"))?;
            Ok(Some(parsed))
        } else {
            Ok(None)
        }
    }

    fn env_u32(key: &str) -> Result<Option<u32>> {
        if let Some(value) = Self::env_string(key)? {
            let parsed = value
                .parse::<u32>()
                .with_context(|| format!("Failed to parse {key} as u32"))?;
            Ok(Some(parsed))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock<'a>() -> std::sync::MutexGuard<'a, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    struct EnvGuard {
        saved: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn new(vars: &[(&str, Option<&str>)]) -> Self {
            let saved = vars
                .iter()
                .map(|(key, _)| (key.to_string(), std::env::var(key).ok()))
                .collect::<Vec<_>>();
            for (key, value) in vars {
                match value {
                    Some(val) => unsafe { std::env::set_var(key, val) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                match value {
                    Some(val) => unsafe { std::env::set_var(key, val) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
        }
    }

    #[test]
    fn load_from_env_only() {
        let _lock = env_lock();
        let temp_home = TempDir::new().unwrap();
        let home = temp_home.path().to_str().unwrap().to_string();

        let _env = EnvGuard::new(&[
            ("HOME", Some(home.as_str())),
            ("OPENROUTER_API_KEY", Some("env-key")),
            ("LI_TIMEOUT_SECS", Some("45")),
            ("LI_MAX_TOKENS", Some("4096")),
            ("LI_CLASSIFIER_MODEL", Some("env-classifier")),
            ("LI_PLANNER_MODEL", Some("env-planner")),
        ]);

        let config = Config::load().unwrap();
        assert_eq!(config.api_key, "env-key");
        assert_eq!(config.timeout_secs, 45);
        assert_eq!(config.max_tokens, 4096);
        assert_eq!(config.classifier_model, "env-classifier");
        assert_eq!(config.planner_model, "env-planner");
    }

    #[test]
    fn load_prefers_env_over_file() {
        let _lock = env_lock();
        let temp_home = TempDir::new().unwrap();
        let home = temp_home.path().to_str().unwrap().to_string();
        let config_dir = temp_home.path().join(".li");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(
            config_dir.join("config"),
            r#"{
                "openrouter_api_key": "file-key",
                "timeout_secs": 20,
                "max_tokens": 1024,
                "classifier_model": "file-classifier",
                "planner_model": "file-planner"
            }"#,
        )
        .unwrap();

        let _env = EnvGuard::new(&[
            ("HOME", Some(home.as_str())),
            ("OPENROUTER_API_KEY", Some("env-key")),
            ("LI_TIMEOUT_SECS", Some("40")),
            ("LI_MAX_TOKENS", None),
            ("LI_CLASSIFIER_MODEL", None),
            ("LI_PLANNER_MODEL", Some("env-planner")),
        ]);

        let config = Config::load().unwrap();
        assert_eq!(config.api_key, "env-key");
        assert_eq!(config.timeout_secs, 40);
        assert_eq!(config.max_tokens, 1024);
        assert_eq!(config.classifier_model, "file-classifier");
        assert_eq!(config.planner_model, "env-planner");
    }

    #[test]
    fn load_errors_without_api_key() {
        let _lock = env_lock();
        let temp_home = TempDir::new().unwrap();
        let home = temp_home.path().to_str().unwrap().to_string();

        let _env = EnvGuard::new(&[
            ("HOME", Some(home.as_str())),
            ("OPENROUTER_API_KEY", None),
            ("LI_TIMEOUT_SECS", None),
            ("LI_MAX_TOKENS", None),
            ("LI_CLASSIFIER_MODEL", None),
            ("LI_PLANNER_MODEL", None),
        ]);

        let err = Config::load().unwrap_err();
        assert!(err.to_string().contains("OpenRouter API key not found"));
    }

}
