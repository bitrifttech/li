use anyhow::{Context, Result, anyhow};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub(crate) const DEFAULT_MAX_TOKENS: u32 = 2048;
const DEFAULT_PLANNER_MODEL: &str = "minimax/minimax-m2:free";
const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_CEREBRAS_BASE_URL: &str = "https://api.cerebras.ai/v1";

fn default_user_agent() -> String {
    format!("li/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Clone)]
pub struct Config {
    pub llm: LlmSettings,
    pub models: ModelSettings,
    pub recovery: RecoverySettings,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let mut path = home_dir().context("Could not determine home directory")?;
        path.push(".li/config");
        Ok(path)
    }

    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let mut builder = ConfigBuilder::new();

        if path.exists() {
            builder = Self::apply_file(builder, &path)?;
        }

        builder = apply_env_overrides(builder)?;

        let config = builder.build()?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Unable to create config directory {}", parent.display())
            })?;
        }

        let payload = PersistedConfig::from(self);
        let json = serde_json::to_string_pretty(&payload)
            .context("Failed to serialize configuration to JSON")?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.llm.api_key.trim().is_empty() {
            let provider = self.llm.provider;
            let env_var = provider.api_key_env_var();
            Err(anyhow!(
                "{} API key not found. Set {} or add it to {}",
                provider.display_name(),
                env_var,
                Self::config_path()?.display()
            ))
        } else {
            Ok(())
        }
    }

    fn apply_file(mut builder: ConfigBuilder, path: &Path) -> Result<ConfigBuilder> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed reading config at {}", path.display()))?;

        if contents.trim().is_empty() {
            return Ok(builder);
        }

        let raw: RawConfig = serde_json::from_str(&contents)
            .with_context(|| format!("Failed parsing JSON config at {}", path.display()))?;

        builder = match raw {
            RawConfig::Nested(cfg) => cfg.apply(builder),
            RawConfig::Legacy(cfg) => cfg.apply(builder),
        };

        Ok(builder)
    }
}

#[derive(Debug, Clone)]
pub struct LlmSettings {
    pub provider: LlmProvider,
    pub api_key: String,
    pub timeout_secs: u64,
    pub base_url: String,
    pub user_agent: String,
}

impl Default for LlmSettings {
    fn default() -> Self {
        let provider = LlmProvider::OpenRouter;
        Self {
            provider,
            api_key: String::new(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            base_url: provider.default_base_url().to_string(),
            user_agent: default_user_agent(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LlmProvider {
    OpenRouter,
    Cerebras,
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmProvider::OpenRouter => write!(f, "openrouter"),
            LlmProvider::Cerebras => write!(f, "cerebras"),
        }
    }
}

impl FromStr for LlmProvider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openrouter" => Ok(LlmProvider::OpenRouter),
            "cerebras" => Ok(LlmProvider::Cerebras),
            other => Err(anyhow!("Unknown LLM provider '{other}'")),
        }
    }
}

impl LlmProvider {
    pub fn default_base_url(self) -> &'static str {
        match self {
            LlmProvider::OpenRouter => DEFAULT_OPENROUTER_BASE_URL,
            LlmProvider::Cerebras => DEFAULT_CEREBRAS_BASE_URL,
        }
    }

    pub fn api_key_env_var(self) -> &'static str {
        match self {
            LlmProvider::OpenRouter => "OPENROUTER_API_KEY",
            LlmProvider::Cerebras => "CEREBRAS_API_KEY",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            LlmProvider::OpenRouter => "OpenRouter",
            LlmProvider::Cerebras => "Cerebras",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelSettings {
    pub planner: String,
    pub max_tokens: u32,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            planner: DEFAULT_PLANNER_MODEL.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecoverySettings {
    pub enabled: bool,
    pub preference: RecoveryPreference,
    pub auto_install: bool,
}

impl Default for RecoverySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preference: RecoveryPreference::NeverRecover,
            auto_install: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryPreference {
    AlternativesFirst,
    InstallationFirst,
    SkipOnError,
    NeverRecover,
}

impl fmt::Display for RecoveryPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            RecoveryPreference::AlternativesFirst => "alternatives-first",
            RecoveryPreference::InstallationFirst => "installation-first",
            RecoveryPreference::SkipOnError => "skip-on-error",
            RecoveryPreference::NeverRecover => "never-recover",
        };
        write!(f, "{label}")
    }
}

impl FromStr for RecoveryPreference {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "alternatives-first" => Ok(RecoveryPreference::AlternativesFirst),
            "installation-first" => Ok(RecoveryPreference::InstallationFirst),
            "skip-on-error" => Ok(RecoveryPreference::SkipOnError),
            "never-recover" => Ok(RecoveryPreference::NeverRecover),
            other => Err(anyhow!("Unknown recovery preference '{other}'")),
        }
    }
}

#[derive(Debug)]
pub struct ConfigBuilder {
    llm: LlmSettings,
    models: ModelSettings,
    recovery: RecoverySettings,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            llm: LlmSettings::default(),
            models: ModelSettings::default(),
            recovery: RecoverySettings::default(),
        }
    }

    pub fn with_llm<F>(mut self, update: F) -> Self
    where
        F: FnOnce(&mut LlmSettings),
    {
        update(&mut self.llm);
        self
    }

    pub fn with_models<F>(mut self, update: F) -> Self
    where
        F: FnOnce(&mut ModelSettings),
    {
        update(&mut self.models);
        self
    }

    pub fn with_recovery<F>(mut self, update: F) -> Self
    where
        F: FnOnce(&mut RecoverySettings),
    {
        update(&mut self.recovery);
        self
    }

    pub fn build(self) -> Result<Config> {
        Ok(Config {
            llm: self.llm,
            models: self.models,
            recovery: self.recovery,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawConfig {
    Nested(FileConfigV2),
    Legacy(FileConfigV1),
}

#[derive(Debug, Deserialize)]
struct FileConfigV1 {
    openrouter_api_key: Option<String>,
    timeout_secs: Option<u64>,
    max_tokens: Option<u32>,
    planner_model: Option<String>,
}

impl FileConfigV1 {
    fn apply(self, builder: ConfigBuilder) -> ConfigBuilder {
        builder
            .with_llm(|llm| {
                if let Some(api_key) = self.openrouter_api_key.clone() {
                    llm.api_key = api_key;
                }
                if let Some(timeout) = self.timeout_secs {
                    llm.timeout_secs = timeout;
                }
            })
            .with_models(|models| {
                if let Some(max_tokens) = self.max_tokens {
                    models.max_tokens = max_tokens;
                }
                if let Some(planner) = self.planner_model.clone() {
                    models.planner = planner;
                }
            })
    }
}

#[derive(Debug, Deserialize)]
struct FileConfigV2 {
    llm: FileLlmSettings,
    models: FileModelSettings,
    #[serde(default)]
    recovery: Option<FileRecoverySettings>,
}

impl FileConfigV2 {
    fn apply(self, builder: ConfigBuilder) -> ConfigBuilder {
        let builder = builder.with_llm(|llm| {
            if let Some(provider) = self.llm.provider.clone() {
                if let Ok(parsed) = provider.parse::<LlmProvider>() {
                    if llm.provider != parsed {
                        llm.provider = parsed;
                        llm.base_url = parsed.default_base_url().to_string();
                    }
                }
            }
            if let Some(api_key) = self.llm.api_key.clone() {
                llm.api_key = api_key;
            }
            if let Some(timeout) = self.llm.timeout_secs {
                llm.timeout_secs = timeout;
            }
            if let Some(base_url) = self.llm.base_url.clone() {
                llm.base_url = base_url;
            }
            if let Some(user_agent) = self.llm.user_agent.clone() {
                llm.user_agent = user_agent;
            }
        });

        let builder = builder.with_models(|models| {
            if let Some(planner) = self.models.planner.clone() {
                models.planner = planner;
            }
            if let Some(max_tokens) = self.models.max_tokens {
                models.max_tokens = max_tokens;
            }
        });

        if let Some(recovery) = self.recovery {
            builder.with_recovery(|settings| {
                if let Some(enabled) = recovery.enabled {
                    settings.enabled = enabled;
                }
                if let Some(preference) = recovery.preference {
                    settings.preference = preference;
                }
                if let Some(auto_install) = recovery.auto_install {
                    settings.auto_install = auto_install;
                }
            })
        } else {
            builder
        }
    }
}

#[derive(Debug, Deserialize)]
struct FileLlmSettings {
    provider: Option<String>,
    api_key: Option<String>,
    timeout_secs: Option<u64>,
    base_url: Option<String>,
    user_agent: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileModelSettings {
    planner: Option<String>,
    max_tokens: Option<u32>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct FileRecoverySettings {
    enabled: Option<bool>,
    preference: Option<RecoveryPreference>,
    auto_install: Option<bool>,
}

#[derive(Serialize)]
struct PersistedConfig<'a> {
    llm: PersistedLlm<'a>,
    models: PersistedModels<'a>,
    recovery: PersistedRecovery,
}

impl<'a> From<&'a Config> for PersistedConfig<'a> {
    fn from(config: &'a Config) -> Self {
        PersistedConfig {
            llm: PersistedLlm {
                provider: config.llm.provider,
                api_key: &config.llm.api_key,
                timeout_secs: config.llm.timeout_secs,
                base_url: &config.llm.base_url,
                user_agent: &config.llm.user_agent,
            },
            models: PersistedModels {
                planner: &config.models.planner,
                max_tokens: config.models.max_tokens,
            },
            recovery: PersistedRecovery {
                enabled: config.recovery.enabled,
                preference: config.recovery.preference,
                auto_install: config.recovery.auto_install,
            },
        }
    }
}

#[derive(Serialize)]
struct PersistedLlm<'a> {
    provider: LlmProvider,
    api_key: &'a str,
    timeout_secs: u64,
    base_url: &'a str,
    user_agent: &'a str,
}

#[derive(Serialize)]
struct PersistedModels<'a> {
    planner: &'a str,
    max_tokens: u32,
}

#[derive(Serialize)]
struct PersistedRecovery {
    enabled: bool,
    preference: RecoveryPreference,
    auto_install: bool,
}

fn apply_env_overrides(mut builder: ConfigBuilder) -> Result<ConfigBuilder> {
    if let Some(provider_raw) = env_string("LI_PROVIDER")? {
        let provider = provider_raw
            .parse::<LlmProvider>()
            .with_context(|| format!("Failed to parse LI_PROVIDER value '{provider_raw}'"))?;
        builder = builder.with_llm(|llm| {
            if llm.provider != provider {
                llm.provider = provider;
                llm.base_url = provider.default_base_url().to_string();
            }
        });
    }

    if let Some(base_url) = env_string("LI_LLM_BASE_URL")? {
        builder = builder.with_llm(|llm| llm.base_url = base_url.clone());
    }

    if let Some(api_key) = env_string("OPENROUTER_API_KEY")? {
        builder = builder.with_llm(|llm| {
            if llm.provider == LlmProvider::OpenRouter {
                llm.api_key = api_key.clone();
            }
        });
    }

    if let Some(api_key) = env_string("CEREBRAS_API_KEY")? {
        builder = builder.with_llm(|llm| {
            if llm.provider == LlmProvider::Cerebras {
                llm.api_key = api_key.clone();
            }
        });
    }

    if let Some(timeout) = env_u64("LI_TIMEOUT_SECS")? {
        builder = builder.with_llm(|llm| llm.timeout_secs = timeout);
    }

    if let Some(max_tokens) = env_u32("LI_MAX_TOKENS")? {
        builder = builder.with_models(|models| models.max_tokens = max_tokens);
    }

    if let Some(planner) = env_string("LI_PLANNER_MODEL")? {
        builder = builder.with_models(|models| models.planner = planner);
    }

    Ok(builder)
}

fn env_string(key: &str) -> Result<Option<String>> {
    match env::var(key) {
        Ok(val) => Ok(Some(val)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(anyhow!("{key} contains invalid UTF-8")),
    }
}

fn env_u64(key: &str) -> Result<Option<u64>> {
    if let Some(value) = env_string(key)? {
        let parsed = value
            .parse::<u64>()
            .with_context(|| format!("Failed to parse {key} as u64"))?;
        Ok(Some(parsed))
    } else {
        Ok(None)
    }
}

fn env_u32(key: &str) -> Result<Option<u32>> {
    if let Some(value) = env_string(key)? {
        let parsed = value
            .parse::<u32>()
            .with_context(|| format!("Failed to parse {key} as u32"))?;
        Ok(Some(parsed))
    } else {
        Ok(None)
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
            ("LI_PLANNER_MODEL", Some("env-planner")),
        ]);

        let config = Config::load().unwrap();
        assert_eq!(config.llm.api_key, "env-key");
        assert_eq!(config.llm.timeout_secs, 45);
        assert_eq!(config.models.max_tokens, 4096);
        assert_eq!(config.models.planner, "env-planner");
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
                "planner_model": "file-planner"
            }"#,
        )
        .unwrap();

        let _env = EnvGuard::new(&[
            ("HOME", Some(home.as_str())),
            ("OPENROUTER_API_KEY", Some("env-key")),
            ("LI_TIMEOUT_SECS", Some("40")),
            ("LI_MAX_TOKENS", None),
            ("LI_PLANNER_MODEL", Some("env-planner")),
        ]);

        let config = Config::load().unwrap();
        assert_eq!(config.llm.api_key, "env-key");
        assert_eq!(config.llm.timeout_secs, 40);
        assert_eq!(config.models.max_tokens, 1024);
        assert_eq!(config.models.planner, "env-planner");
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
            ("LI_PLANNER_MODEL", None),
        ]);

        let err = Config::load().unwrap_err();
        assert!(err.to_string().contains("OpenRouter API key not found"));
    }

    #[test]
    fn load_supports_cerebras_provider() {
        let _lock = env_lock();
        let temp_home = TempDir::new().unwrap();
        let home = temp_home.path().to_str().unwrap().to_string();

        let _env = EnvGuard::new(&[
            ("HOME", Some(home.as_str())),
            ("LI_PROVIDER", Some("cerebras")),
            ("CEREBRAS_API_KEY", Some("cb-key")),
        ]);

        let config = Config::load().unwrap();
        assert_eq!(config.llm.provider, LlmProvider::Cerebras);
        assert_eq!(config.llm.api_key, "cb-key");
        assert_eq!(config.llm.base_url, DEFAULT_CEREBRAS_BASE_URL);
    }

    #[test]
    fn save_persists_nested_structure() {
        let _lock = env_lock();
        let temp_home = TempDir::new().unwrap();
        let home = temp_home.path().to_str().unwrap().to_string();

        let _env = EnvGuard::new(&[("HOME", Some(home.as_str()))]);

        let mut config = Config::builder().build().unwrap();
        config.llm.api_key = "test-key".to_string();
        config.llm.timeout_secs = 55;
        config.models.max_tokens = 999;
        config.models.planner = "custom/planner".to_string();
        config.recovery.enabled = true;
        config.recovery.preference = RecoveryPreference::SkipOnError;
        config.recovery.auto_install = true;
        config.save().unwrap();

        let persisted = std::fs::read_to_string(Config::config_path().unwrap()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&persisted).unwrap();
        assert_eq!(json["llm"]["api_key"], "test-key");
        assert_eq!(json["llm"]["timeout_secs"], 55);
        assert_eq!(json["models"]["planner"], "custom/planner");
        assert_eq!(json["models"]["max_tokens"], 999);
        assert_eq!(json["recovery"]["enabled"], true);
        assert_eq!(json["recovery"]["preference"], "skip-on-error");
        assert_eq!(json["recovery"]["auto_install"], true);
    }
}
