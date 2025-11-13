#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    use crate::config::{Config, LlmProvider, DEFAULT_CEREBRAS_BASE_URL};
    use crate::config::builder::ConfigBuilder;
    use crate::config::environment::{env_string, env_u64, env_u32};

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
        config.recovery.enabled = false;
        config.save().unwrap();

        let persisted = std::fs::read_to_string(Config::config_path().unwrap()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&persisted).unwrap();
        assert_eq!(json["llm"]["api_key"], "test-key");
        assert_eq!(json["llm"]["timeout_secs"], 55);
        assert_eq!(json["models"]["planner"], "custom/planner");
        assert_eq!(json["models"]["max_tokens"], 999);
        assert_eq!(json["recovery"]["enabled"], false);
    }

    #[test]
    fn test_env_string() {
        let _lock = env_lock();
        let _env = EnvGuard::new(&[("TEST_VAR", Some("test_value"))]);
        
        assert_eq!(env_string("TEST_VAR").unwrap(), Some("test_value".to_string()));
        assert_eq!(env_string("NONEXISTENT_VAR").unwrap(), None);
    }

    #[test]
    fn test_env_u64() {
        let _lock = env_lock();
        let _env = EnvGuard::new(&[("TEST_U64", Some("123"))]);
        
        assert_eq!(env_u64("TEST_U64").unwrap(), Some(123));
        assert_eq!(env_u64("NONEXISTENT_VAR").unwrap(), None);
    }

    #[test]
    fn test_env_u32() {
        let _lock = env_lock();
        let _env = EnvGuard::new(&[("TEST_U32", Some("456"))]);
        
        assert_eq!(env_u32("TEST_U32").unwrap(), Some(456));
        assert_eq!(env_u32("NONEXISTENT_VAR").unwrap(), None);
    }
}
