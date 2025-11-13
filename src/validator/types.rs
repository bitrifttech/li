use std::collections::HashMap;

/// Main command validator structure
pub struct CommandValidator {
    pub cache: HashMap<String, bool>,
}

/// Result of command validation
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    pub missing_commands: Vec<MissingCommand>,
    pub plan_can_continue: bool,
}

/// Information about a missing command
#[derive(Debug, Clone, PartialEq)]
pub struct MissingCommand {
    pub command: String,
    pub failed_command_line: String,
    pub plan_step: usize,
    pub is_dry_run: bool,
}

impl Default for CommandValidator {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
}