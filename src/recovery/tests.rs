#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::recovery::{
        CommandAlternative, InstallationInstruction, RecoveryEngine, RecoveryOptions,
    };
    use crate::validator::MissingCommand;

    #[test]
    fn command_alternative_display_includes_details() {
        let alt = CommandAlternative {
            command: "brew install foo".to_string(),
            description: "Install using Homebrew".to_string(),
            confidence: 0.42,
        };

        let rendered = alt.to_string();
        assert!(rendered.contains("brew install foo"));
        assert!(rendered.contains("Install using Homebrew"));
    }

    #[test]
    fn installation_instruction_display_lists_commands() {
        let instruction = InstallationInstruction {
            command: "foo".to_string(),
            install_commands: vec![
                "brew install foo".to_string(),
                "apt-get install foo".to_string(),
            ],
            package_managers: vec!["brew".to_string(), "apt".to_string()],
            confidence: 0.7,
        };

        let rendered = instruction.to_string();
        assert!(rendered.contains("brew install foo"));
        assert!(rendered.contains("apt-get install foo"));
    }

    #[test]
    fn recovery_options_skip_only_defaults_to_skip_on_error() {
        let options = RecoveryOptions::skip_only();
        assert!(options.command_alternatives.is_empty());
        assert!(options.installation_instructions.is_empty());
        assert!(options.can_skip_step);
        assert!(!options.retry_possible);
    }

    #[test]
    fn recovery_engine_respects_enabled_flag() {
        let missing = MissingCommand {
            command: "fake".to_string(),
            failed_command_line: "fake --flag".to_string(),
            plan_step: 0,
            is_dry_run: false,
        };

        let mut disabled_config = Config::builder().build().unwrap();
        disabled_config.llm.api_key = "test-key".to_string();
        disabled_config.recovery.enabled = false;
        let engine = RecoveryEngine::new(&disabled_config).expect("engine should construct");
        assert!(!engine.should_attempt_recovery(&missing));

        let mut enabled_config = Config::builder().build().unwrap();
        enabled_config.llm.api_key = "test-key".to_string();
        enabled_config.recovery.enabled = true;
        let engine = RecoveryEngine::new(&enabled_config).expect("engine should construct");
        assert!(engine.should_attempt_recovery(&missing));
    }
}
