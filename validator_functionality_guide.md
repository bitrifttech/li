# Validator Module Functionality Guide

## What This File Does

The [`validator/mod.rs`](src/validator/mod.rs) file implements a **command validation system** that checks whether all commands in an execution plan are available on the current system before attempting to run them. This prevents execution failures due to missing tools.

Think of it like a "pre-flight check" for shell commands - similar to how Python's `import` checks if modules exist, or how C++ linkers check if libraries are available.

## Core Architecture

The module centers around the **CommandValidator** struct, which maintains a cache of command existence checks to avoid repeated system calls.

### Primary Workflow

1. **Create validator** → Initialize with empty cache
2. **Extract commands** → Parse complex command lines to get primary tool names
3. **Check existence** → Verify each command is available on the system
4. **Aggregate results** → Collect missing commands and determine if execution can continue
5. **Return validation report** → Provide detailed feedback about what's missing

## Method Execution Flow

### 1. CommandValidator::new()
**When called:** At the beginning of validation process
**What it does:** 
- Creates a new CommandValidator instance
- Initializes empty HashMap cache for performance
- Equivalent to `CommandValidator()` in Python or default constructor in C++

### 2. extract_command(cmd: &str) -> Option<String>
**When called:** Whenever the validator needs to process a command line
**What it does:**
- **Input:** Complex command string like `git commit && npm test`
- **Processing:**
  - Trims whitespace from the command
  - Handles shell operators (`&&`, `||`, `|`, `;`) by taking the first part
  - Extracts the first token (actual command name)
  - Removes path prefixes (`./`, `/`, `~/`) to get clean command name
- **Output:** `Some("git")` for `git commit --help`, `None` for empty string
- **Example transformations:**
  - `"docker run nginx && systemctl status docker"` → `"docker"`
  - `"npm test --coverage"` → `"npm"`
  - `"./build.sh && deploy"` → `"build.sh"`
  - `"python -m pip install"` → `"python"`

### 3. command_exists(&mut self, cmd: &str) -> bool
**When called:** After extracting a command name from a command line
**What it does:**
- **Caching:** First checks cache (`self.cache.get(cmd)`) to avoid repeated system calls
- **Path commands:** If command starts with `/`, `./`, or `~/`:
  - Expands `~` to home directory
  - Checks if file exists AND has executable permissions using filesystem metadata
- **Regular commands:** Uses shell command `command -v <cmd>` to check PATH
- **Fallback method:** If `command -v` fails, tries `<cmd> --version` as backup
- **Caching result:** Stores result in cache for future lookups
- **Returns:** `true` if command is found and executable, `false` otherwise

### 4. check_command_existence(&self, cmd: &str) -> bool
**When called:** Internally by `command_exists()` (not called directly by users)
**What it does:**
- The actual implementation of command checking logic
- Handles both path-based commands and PATH-based commands
- Performs filesystem checks or shell command execution
- This is separated from `command_exists()` to allow caching logic to be isolated

### 5. validate_plan(&mut self, plan: &Plan) -> Result<ValidationResult>
**When called:** Main entry point for validating an entire execution plan
**What it does:**
- **Input:** A Plan struct containing `dry_run_commands` and `execute_commands` vectors
- **Processing:**
  1. Iterates through all dry-run commands with their indices
  2. For each command line, extracts the primary command using `extract_command()`
  3. Checks if command exists using `command_exists()`
  4. If missing, creates a `MissingCommand` struct with details
  5. Repeats the same process for execute commands
- **Decision logic:**
  - Plan can continue if no commands are missing
  - OR if only dry-run commands are missing (execution commands must exist)
  - If any execute command is missing, plan cannot continue
- **Output:** `ValidationResult` containing vector of missing commands and boolean decision

### 6. check_single_command(&mut self, cmd_line: &str) -> Result<bool>
**When called:** For validating individual command lines (not entire plans)
**What it does:**
- Extracts command from command line
- Returns error if cannot extract command
- Calls `command_exists()` and returns the boolean result
- Simpler version of `validate_plan()` for single command validation

### 7. get_available_tools(&self) -> Vec<String>
**When called:** To get a list of commonly available tools on the system
**What it does:**
- Contains a hardcoded list of common development and system tools
- Creates a new validator instance (to avoid cache pollution)
- Checks each tool using `command_exists()`
- Returns vector of tool names that are available
- **Use case:** Could be used for auto-completion, suggestions, or system capability reporting

### 8. clear_cache(&mut self)
**When called:** To reset the validator's internal state
**What it does:**
- Clears the command existence cache
- Useful when system PATH changes or tools are installed/uninstalled during runtime

### 9. cache_stats(&self) -> (usize, usize)
**When called:** For debugging or performance monitoring
**What it does:**
- Returns tuple of (total cached commands, commands that were found)
- Helps identify cache hit rates and validator efficiency

## Typical Usage Pattern

```rust
// Create validator
let mut validator = CommandValidator::new();

// Validate an entire plan
let result = validator.validate_plan(&execution_plan)?;

match result.plan_can_continue {
    true => println("Plan can proceed"),
    false => {
        for missing in result.missing_commands {
            eprintln!("Missing command: {} at step {}", 
                     missing.command, missing.plan_step);
        }
    }
}

// Or check single command
if !validator.check_single_command("npm test")? {
    println("npm is not available");
}
```

## Performance Considerations

1. **Caching:** Commands are checked once per validator instance, then cached
2. **Async operations:** All system calls are non-blocking using tokio
3. **Minimizing system calls:** Cache prevents repeated `command -v` calls
4. **Fresh instances:** `get_available_tools()` creates new validator to avoid cache bloat

## Integration Context

This validator is typically called:
- **Before plan execution** to ensure all required tools are available
- **During plan creation** to provide feedback about missing dependencies
- **For system introspection** to discover available capabilities

The validation results can be used to:
- Stop execution early if critical tools are missing
- Provide helpful error messages to users
- Suggest alternative commands or installation instructions
- Make decisions about fallback execution strategies

This design provides a robust, user-friendly way to handle command dependencies in automated execution environments.