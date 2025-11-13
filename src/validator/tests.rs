#[cfg(test)]
mod tests {
    use crate::validator::{CommandValidator, MissingCommand, ValidationResult};
    use crate::planner;

    #[test]
    fn test_extract_command() {
        // Simple commands
        assert_eq!(
            CommandValidator::extract_command("ls"),
            Some("ls".to_string())
        );
        assert_eq!(
            CommandValidator::extract_command("git status"),
            Some("git".to_string())
        );
        assert_eq!(
            CommandValidator::extract_command("npm install"),
            Some("npm".to_string())
        );
        assert_eq!(CommandValidator::extract_command("ls -la"), Some("ls".to_string()));
        assert_eq!(CommandValidator::extract_command("git status"), Some("git".to_string()));
        assert_eq!(CommandValidator::extract_command("docker run nginx"), Some("docker".to_string()));
        assert_eq!(CommandValidator::extract_command("/usr/bin/find . -name '*.rs'"), Some("/usr/bin/find".to_string()));
        assert_eq!(CommandValidator::extract_command("echo 'hello world'"), Some("echo".to_string()));

        // Complex shell constructs
        assert_eq!(
            CommandValidator::extract_command("git add . && git commit"),
            Some("git".to_string())
        );
        assert_eq!(
            CommandValidator::extract_command("make build || echo failed"),
            Some("make".to_string())
        );
        assert_eq!(
            CommandValidator::extract_command("ps aux | grep node"),
            Some("ps".to_string())
        );
        assert_eq!(
            CommandValidator::extract_command("cd /tmp && ls -la"),
            Some("cd".to_string())
        );

        // Path-based commands
        assert_eq!(
            CommandValidator::extract_command("./script.sh"),
            Some("script.sh".to_string())
        );
        assert_eq!(
            CommandValidator::extract_command("/usr/local/bin/custom"),
            Some("/usr/local/bin/custom".to_string())
        );
        assert_eq!(
            CommandValidator::extract_command("~/bin/mytool"),
            Some("~/bin/mytool".to_string())
        );

        // Edge cases
        assert_eq!(CommandValidator::extract_command(""), None);
        assert_eq!(CommandValidator::extract_command("   "), None);
        assert_eq!(
            CommandValidator::extract_command("  ls  -la  "),
            Some("ls".to_string())
        );
    }

    // Note: should_validate_command method doesn't exist in CommandValidator
    // This test is removed as it's not part of the current implementation

    #[tokio::test]
    async fn test_command_exists() {
        let mut validator = CommandValidator::new();

        // Test with a command that should exist on most systems
        let sh_exists = validator.command_exists("sh").await;
        assert!(sh_exists, "sh command should exist");

        // Test with a command that likely doesn't exist
        let fake_exists = validator
            .command_exists("definitely_not_a_real_command_12345")
            .await;
        assert!(!fake_exists, "fake command should not exist");

        // Test common commands that should exist
        assert!(validator.command_exists("ls").await);
        assert!(validator.command_exists("echo").await);
        assert!(!validator.command_exists("fake_binary_xyz").await);

        // Test caching
        let sh_exists_cached = validator.command_exists("sh").await;
        assert!(sh_exists_cached, "cached result should be the same");

        let stats = validator.cache_stats();
        assert!(stats.0 > 0, "cache should have entries");
    }

    #[tokio::test]
    async fn test_check_single_command() {
        let mut validator = CommandValidator::new();
        
        // Test existing command
        assert!(validator.check_single_command("ls -la").await.unwrap());
        
        // Test non-existing command
        assert!(!validator.check_single_command("fakecommand123").await.unwrap());
        
        // Test built-in (should return true without validation)
        assert!(validator.check_single_command("echo hello").await.unwrap());
    }

    #[tokio::test]
    async fn test_validate_plan() {
        let mut validator = CommandValidator::new();
        
        let plan = planner::Plan {
            dry_run_commands: vec![
                "ls -la".to_string(),
                "fakecommand123".to_string(),
                "echo test".to_string(),
            ],
            execute_commands: vec![
                "git status".to_string(),
                "anotherfakecmd".to_string(),
            ],
            confidence: 0.8,
            notes: "Test plan".to_string(),
        };
        
        let result = validator.validate_plan(&plan).await.unwrap();
        
        // Should find both fake commands
        assert_eq!(result.missing_commands.len(), 2);
        assert!(result.missing_commands.iter().any(|cmd| cmd.command == "fakecommand123"));
        assert!(result.missing_commands.iter().any(|cmd| cmd.command == "anotherfakecmd"));
        
        // Plan should not be able to continue with missing commands
        assert!(!result.plan_can_continue);
    }

    #[test]
    fn test_missing_command_creation() {
        let missing = MissingCommand {
            command: "testcmd".to_string(),
            failed_command_line: "testcmd --option".to_string(),
            plan_step: 2,
            is_dry_run: false,
        };
        
        assert_eq!(missing.command, "testcmd");
        assert_eq!(missing.failed_command_line, "testcmd --option");
        assert_eq!(missing.plan_step, 2);
        assert!(!missing.is_dry_run);
    }

    #[test]
    fn test_validation_result_creation() {
        let missing_cmd = MissingCommand {
            command: "missing".to_string(),
            failed_command_line: "missing --arg".to_string(),
            plan_step: 0,
            is_dry_run: true,
        };
        
        let result = ValidationResult {
            missing_commands: vec![missing_cmd],
            plan_can_continue: false,
        };
        
        assert_eq!(result.missing_commands.len(), 1);
        assert!(!result.plan_can_continue);
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let mut validator = CommandValidator::new();

        // Initially empty cache
        assert_eq!(validator.cache_stats(), (0, 0));

        // Populate cache via the public API
        validator.command_exists("sh").await; // should exist
        validator.command_exists("definitely_not_real_cmd_987654321").await; // should not exist

        let (total, found) = validator.cache_stats();
        assert_eq!(total, 2);
        assert_eq!(found, 1);

        // Clear cache
        validator.clear_cache();
        assert_eq!(validator.cache_stats(), (0, 0));
    }

    #[tokio::test]
    async fn test_caching_behavior() {
        let mut validator = CommandValidator::new();
        
        let cmd = "ls";
        
        // First call should check and cache
        let result1 = validator.command_exists(cmd).await;
        let stats_after_first = validator.cache_stats();
        
        // Second call should use cache
        let result2 = validator.command_exists(cmd).await;
        let stats_after_second = validator.cache_stats();
        
        // Results should be the same
        assert_eq!(result1, result2);
        
        // Cache should have grown after first call but not after second
        assert!(stats_after_first.0 > 0);
        assert_eq!(stats_after_first, stats_after_second);
    }

    #[test]
    fn test_edge_case_commands() {
        // Test commands with special characters
        assert_eq!(CommandValidator::extract_command("command-with-dashes"), Some("command-with-dashes".to_string()));
        assert_eq!(CommandValidator::extract_command("command_with_underscores"), Some("command_with_underscores".to_string()));
        assert_eq!(CommandValidator::extract_command("command.with.dots"), Some("command.with.dots".to_string()));
        
        // Test commands with paths
        assert_eq!(CommandValidator::extract_command("./local-script"), Some("local-script".to_string()));
        assert_eq!(CommandValidator::extract_command("../parent-script"), Some("../parent-script".to_string()));
        assert_eq!(CommandValidator::extract_command("/absolute/path/command"), Some("/absolute/path/command".to_string()));
        
        // Test quoted commands
        assert_eq!(CommandValidator::extract_command("\"quoted command\""), Some("\"quoted".to_string()));
        assert_eq!(CommandValidator::extract_command("'single-quoted'"), Some("'single-quoted'".to_string()));
        
        // Test commands with variables
        // Note: should_validate_command method doesn't exist
    }
}