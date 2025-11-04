# li — Implementation Plan

## Decision Summary

Based on PRD discussion and clarifications:

- **Classifier Model**: llama-3.3-70b on Cerebras
- **Planner Model**: qwen-3-235b on Cerebras  
- **Config Location**: `~/.li/config` (JSON format)
- **Config Contents**: API keys, timeouts, model overrides
- **Shell Execution**: Commands run in user's current shell with inherited env/context
- **Tool Detection**: Always probe before planning (`which git`, etc.)
- **Hook Support**: `li install` and `li uninstall` for zsh
- **Testing Strategy**: Real API for integration tests, unit tests for everything else
- **Context Scope**: Single-request only (no multi-turn memory)
- **Output**: Stream directly to user's terminal (inherit stdio)
- **Cerebras API Endpoint**: `https://api.cerebras.ai/v1/chat/completions`

---

## Phase 1: Project Setup & Configuration

### Milestone 1.1: Rust Project Structure
**Goal**: Basic Rust project with correct dependencies and module layout.

**Tasks**:
- Initialize Cargo project
- Add dependencies to `Cargo.toml`:
  - `clap = { version = "4.5", features = ["derive"] }`
  - `reqwest = { version = "0.12", features = ["json"] }`
  - `tokio = { version = "1.40", features = ["full"] }`
  - `serde = { version = "1.0", features = ["derive"] }`
  - `serde_json = "1.0"`
  - `colored = "2.1"`
  - `anyhow = "1.0"`
  - `dirs = "5.0"`
- Create module structure:
  ```
  src/
    main.rs
    cli.rs
    config.rs
    cerebras.rs
    classifier/
      mod.rs
    planner/
      mod.rs
    exec/
      mod.rs
    hook/
      mod.rs
  ```

**Verification**:
```bash
cd li
cargo build
cargo run -- --help
# Should compile successfully and show help output
```

### Milestone 1.2: Configuration System
**Goal**: Load config from `~/.li/config` with fallback to env vars.

**Implementation** (`src/config.rs`):
- Define `Config` struct:
  ```rust
  pub struct Config {
      pub cerebras_api_key: String,
      pub timeout_secs: u64,
      pub max_tokens: u32,
      pub classifier_model: String,
      pub planner_model: String,
  }
  ```
- Implement `Config::load()`:
  - Try to read `~/.li/config` (JSON)
  - Fall back to env vars: `CEREBRAS_API_KEY`, `LI_TIMEOUT_SECS`, etc.
  - Provide defaults: timeout=30, max_tokens=2048
- Handle missing API key gracefully (error message)

**Verification**:
```bash
# Create test config
mkdir -p ~/.li
cat > ~/.li/config << 'EOF'
{
  "cerebras_api_key": "test-key-123",
  "timeout_secs": 30,
  "max_tokens": 2048,
  "classifier_model": "llama-3.3-70b",
  "planner_model": "qwen-3-235b"
}
EOF

# Test config loading
cargo run -- --help
# Should not error about missing API key

# Test env var override
CEREBRAS_API_KEY=override-key cargo run -- --help
# Should prefer env var over file
```

---

## Phase 2: Cerebras API Client

### Milestone 2.1: HTTP Client Foundation
**Goal**: Async HTTP client that can call Cerebras API.

**Implementation** (`src/cerebras.rs`):
- Define `CerebrasClient` struct with API key and base URL (`https://api.cerebras.ai/v1/chat/completions`)
- Implement request/response types:
  ```rust
  pub struct ChatRequest {
      pub model: String,
      pub messages: Vec<Message>,
      pub max_tokens: u32,
      pub temperature: f32,
  }
  
  pub struct ChatResponse {
      pub choices: Vec<Choice>,
  }
  ```
- Implement `CerebrasClient::chat_completion()` method that:
  - Issues a POST to `https://api.cerebras.ai/v1/chat/completions`
  - Sends JSON body `{ "model": ..., "messages": [...] }`
  - Includes headers `Authorization: Bearer <API KEY>` and `Content-Type: application/json`
- Handle timeouts and network errors
- Add retry logic (1 retry on failure)

**Verification**:
```bash
# Add test function in main.rs that calls Cerebras
cargo run -- --test-api
# Should successfully call API and print response
# (requires valid CEREBRAS_API_KEY)

# Optional cURL verification using the same endpoint
curl --location 'https://api.cerebras.ai/v1/chat/completions' \
  --header 'Content-Type: application/json' \
  --header "Authorization: Bearer ${CEREBRAS_API_KEY}" \
  --data '{
    "model": "llama-3.3-70b",
    "messages": [
      {"role": "user", "content": "Tell me a fun fact about space."}
    ]
  }'
```

### Milestone 2.2: JSON Response Parsing
**Goal**: Strict JSON schema validation with helpful errors.

**Implementation**:
- Add JSON validation for responses
- Implement parse-retry logic (1 retry on malformed JSON)
- Clear error messages for schema violations

**Verification**:
```bash
# Unit test with mock JSON responses
cargo test cerebras::tests::parse_valid_json
cargo test cerebras::tests::parse_invalid_json
cargo test cerebras::tests::retry_on_malformed
```

---

## Phase 3: Classifier

### Milestone 3.1: Classifier Types & Prompt
**Goal**: Classification logic with strict two-value output.

**Implementation** (`src/classifier/mod.rs`):
- Define enum:
  ```rust
  pub enum Classification {
      Terminal,
      NaturalLanguage,
  }
  ```
- Define system prompt (from PRD section 8.1)
- Implement response parsing that only accepts `{"type":"NL"}` or `{"type":"TERMINAL"}`
- Reject any other JSON structure

**Implementation** (`src/classifier/prompt.rs`):
```rust
pub fn build_prompt(input: &str) -> Vec<Message> {
    vec![
        Message {
            role: "system",
            content: r#"You are part of a CLI terminal system where users mix natural language with shell commands.
Your job: output **only** a JSON object with the key "type" and value "NL" or "TERMINAL".
If the input is plain English intent, output "NL".
If the input is a valid shell command sequence to run as-is, output "TERMINAL".
Output **exactly** one JSON object, no extra keys, no prose."#,
        },
        Message {
            role: "user",
            content: input,
        },
    ]
}
```

**Verification**:
```bash
# Add classify subcommand for testing
cargo run -- classify "make a new git repo"
# Expected output: NL

cargo run -- classify "git init"
# Expected output: TERMINAL

cargo run -- classify "ls -la"
# Expected output: TERMINAL

cargo run -- classify "show me the largest files"
# Expected output: NL
```

### Milestone 3.2: Classifier Integration
**Goal**: Wire classifier into main CLI flow.

**Implementation**:
- Add `--classify-only` flag
- Call classifier via Cerebras client
- Return appropriate exit codes:
  - Exit code 0: Natural Language
  - Exit code 100: Terminal command
  - Exit code 1: Error

**Verification**:
```bash
cargo run -- --classify-only "git status"
echo $?  # Should be 100

cargo run -- --classify-only "create a new directory called test"
echo $?  # Should be 0

# Test with actual API
CEREBRAS_API_KEY=your-key cargo run -- --classify-only "list files"
```

---

## Phase 4: Planner

### Milestone 4.1: Tool Probing
**Goal**: Detect available tools on the system.

**Implementation** (`src/planner/probe.rs`):
```rust
pub struct SystemContext {
    pub cwd: PathBuf,
    pub os: String,
    pub shell: String,
    pub available_tools: Vec<String>,
}

pub fn probe_system() -> SystemContext {
    let tools = vec!["git", "docker", "brew", "npm", "python", "node"];
    let available = tools.iter()
        .filter(|tool| Command::new("which")
            .arg(tool)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false))
        .map(|s| s.to_string())
        .collect();
    
    SystemContext {
        cwd: env::current_dir().unwrap(),
        os: env::consts::OS.to_string(),
        shell: env::var("SHELL").unwrap_or_default(),
        available_tools: available,
    }
}
```

**Verification**:
```bash
# Add probe subcommand
cargo run -- probe
# Should output:
# CWD: /Users/matthew/...
# OS: macos
# Shell: /bin/zsh
# Available tools: git, brew, python, node
```

### Milestone 4.2: Planner Prompt & Schema
**Goal**: Build planner prompt with context and validate response schema.

**Implementation** (`src/planner/mod.rs`):
- Define `Plan` struct matching PRD schema:
  ```rust
  #[derive(Deserialize, Serialize)]
  pub struct Plan {
      pub r#type: String,  // Must be "plan"
      pub confidence: f64,
      pub dry_run_commands: Vec<String>,
      pub execute_commands: Vec<String>,
      pub notes: String,
  }
  ```
- Build system prompt with context (from PRD section 8.2)
- Validate strict schema (reject additional properties)

**Implementation** (`src/planner/prompt.rs`):
```rust
pub fn build_prompt(request: &str, context: &SystemContext) -> Vec<Message> {
    let system_content = format!(r#"You convert plain English into a **safe, minimal shell plan**.
Follow these rules:

1. Prefer dry_run_commands and idempotent checks first.
2. Avoid destructive operations unless preceded by a safety probe.
3. Keep commands portable for macOS/Linux where possible.
4. Output **only** valid JSON matching the provided schema—no extra fields, no comments, no prose.
5. If the task requires coding or multi-file scaffolding, set notes to explain that v1 doesn't support code generation and produce a minimal plan that stops safely.

Context:
- cwd: {}
- os: {}
- shell: {}
- available_tools: {}

Schema:
{{
  "type": "object",
  "required": ["type","confidence","dry_run_commands","execute_commands","notes"],
  "properties": {{
    "type": {{ "const": "plan" }},
    "confidence": {{ "type": "number", "minimum": 0, "maximum": 1 }},
    "dry_run_commands": {{ "type": "array", "items": {{ "type": "string" }} }},
    "execute_commands": {{ "type": "array", "items": {{ "type": "string" }} }},
    "notes": {{ "type": "string" }}
  }},
  "additionalProperties": false
}}"#,
        context.cwd.display(),
        context.os,
        context.shell,
        context.available_tools.join(", ")
    );

    vec![
        Message { role: "system", content: system_content },
        Message { role: "user", content: request.to_string() },
    ]
}
```

**Verification**:
```bash
# Add plan subcommand (outputs plan JSON, doesn't execute)
cargo run -- plan "make a new git repo"
# Should output valid Plan JSON:
# {
#   "type": "plan",
#   "confidence": 0.85,
#   "dry_run_commands": ["git status"],
#   "execute_commands": ["git init", "git add .", "git commit -m 'Initial commit'"],
#   "notes": "..."
# }

cargo run -- plan "list the 10 largest files"
# Should output plan with du/ls commands
```

### Milestone 4.3: Planner Integration
**Goal**: Full classify → plan pipeline.

**Implementation** (`src/main.rs`):
- Wire classifier and planner together
- Add basic plan preview (text output)
- Don't execute yet

**Verification**:
```bash
cargo run -- "create a new git repository"
# Should show:
# Plan (confidence: 0.85):
#   Dry-run:
#     • git status
#   Will execute:
#     1. git init
#     2. git add .
#     3. git commit -m "Initial commit"
#   Notes: Created minimal git repo with initial commit.
# [Not executing - executor not implemented yet]
```

---

## Phase 5: Executor

### Milestone 5.1: Command Execution
**Goal**: Execute commands sequentially with proper stdio handling.

**Implementation** (`src/exec/mod.rs`):
```rust
pub fn execute_plan(plan: &Plan) -> Result<i32> {
    // Execute dry_run_commands first
    for cmd in &plan.dry_run_commands {
        println!("🔍 Dry-run: {}", cmd);
        let status = execute_command(cmd)?;
        if !status.success() {
            eprintln!("Dry-run failed, aborting");
            return Ok(status.code().unwrap_or(1));
        }
    }

    // Execute actual commands
    for (i, cmd) in plan.execute_commands.iter().enumerate() {
        println!("▶ [{}/{}] {}", i + 1, plan.execute_commands.len(), cmd);
        let status = execute_command(cmd)?;
        if !status.success() {
            eprintln!("Command failed with exit code: {}", status.code().unwrap_or(1));
            return Ok(status.code().unwrap_or(1));
        }
    }

    Ok(0)
}

fn execute_command(cmd: &str) -> Result<ExitStatus> {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(Into::into)
}
```

**Verification**:
```bash
# Test with safe commands
cargo run -- "show current directory"
# Should execute `pwd` and show output directly in terminal

# Test with dry-run
cargo run -- "initialize a git repo"
# Should run git status first, then git init

# Test failure handling
cargo run -- "run a command that does not exist"
# Should stop on first failure and show exit code
```

### Milestone 5.2: Approval Prompt
**Goal**: Interactive y/N prompt before execution.

**Implementation** (`src/exec/mod.rs`):
- Show plan preview with colors
- Prompt `[y/N]:` 
- Only execute on 'y' or 'Y'
- Handle `--yes` flag to skip prompt

**Verification**:
```bash
# Interactive approval
cargo run -- "make a test directory"
# Shows plan, prompts, waits for input
# Type 'y' → executes
# Type 'n' → exits without executing

# Auto-approve with flag
cargo run -- --yes "make a test directory"
# Executes immediately without prompt
```

---

## Phase 6: Shell Hook

### Milestone 6.1: Hook Script Generation
**Goal**: Generate zsh widget code for hook installation.

**Implementation** (`src/hook/zsh.rs`):
```rust
pub fn generate_hook_script() -> String {
    r#"
# li natural language terminal hook
_li_accept_line() {
    # Get current buffer
    local input="$BUFFER"
    
    # Classify the input
    li --classify-only "$input"
    local code=$?
    
    if [[ $code -eq 100 ]]; then
        # TERMINAL - execute normally
        zle .accept-line
    elif [[ $code -eq 0 ]]; then
        # NL - route through li
        zle .kill-whole-line
        print -z "li \"$input\""
        zle .accept-line
    else
        # Error - show message and don't execute
        echo "li classification error"
        zle .accept-line
    fi
}

zle -N _li_accept_line
bindkey '^M' _li_accept_line  # Bind Enter key
"#.to_string()
}
```

**Implementation** (`src/hook/zsh.rs` continued):
```rust
pub fn install() -> Result<()> {
    let home = dirs::home_dir().ok_or(anyhow!("Cannot find home directory"))?;
    let hook_file = home.join(".zshrc.d").join("li.zsh");
    
    // Create .zshrc.d if it doesn't exist
    fs::create_dir_all(hook_file.parent().unwrap())?;
    
    // Write hook script
    fs::write(&hook_file, generate_hook_script())?;
    
    // Add source line to .zshrc if not present
    let zshrc = home.join(".zshrc");
    let source_line = "source ~/.zshrc.d/li.zsh\n";
    
    let mut content = fs::read_to_string(&zshrc).unwrap_or_default();
    if !content.contains("li.zsh") {
        content.push_str(source_line);
        fs::write(&zshrc, content)?;
    }
    
    println!("✓ Installed zsh hook to {}", hook_file.display());
    println!("  Restart your shell or run: source ~/.zshrc");
    
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let home = dirs::home_dir().ok_or(anyhow!("Cannot find home directory"))?;
    let hook_file = home.join(".zshrc.d").join("li.zsh");
    
    // Remove hook file
    if hook_file.exists() {
        fs::remove_file(&hook_file)?;
        println!("✓ Removed {}", hook_file.display());
    }
    
    // Remove source line from .zshrc
    let zshrc = home.join(".zshrc");
    if zshrc.exists() {
        let content = fs::read_to_string(&zshrc)?;
        let new_content = content.lines()
            .filter(|line| !line.contains("li.zsh"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&zshrc, new_content)?;
        println!("✓ Removed hook from .zshrc");
    }
    
    Ok(())
}
```

**Verification**:
```bash
# Install hook
cargo run -- install
# Check that ~/.zshrc.d/li.zsh was created
cat ~/.zshrc.d/li.zsh
# Check that ~/.zshrc sources it
grep li.zsh ~/.zshrc

# Test in new shell
zsh
# Type: "show current directory" <Enter>
# Should classify as NL and route through li

# Type: "ls -la" <Enter>
# Should classify as TERMINAL and execute directly

# Uninstall
cargo run -- uninstall
# Check that hook file is removed
ls ~/.zshrc.d/li.zsh  # Should not exist
```

---

## Phase 7: Polish & Error Handling

### Milestone 7.1: Better Output Formatting
**Goal**: Colored, user-friendly terminal output.

**Implementation**:
- Use `colored` crate for plan preview
- Add emojis/symbols for visual clarity
- Format errors clearly

**Verification**:
```bash
cargo run -- "make a git repo"
# Should show colorful, well-formatted plan

cargo run -- --no-color "make a git repo"
# Should show plain text (no ANSI codes)
```

### Milestone 7.2: Error Handling & Edge Cases
**Goal**: Graceful failures with helpful messages.

**Implementation**:
- Network timeout → clear error message
- Invalid API key → helpful message
- Malformed JSON from API → retry once, then fail gracefully
- Empty input → friendly message
- Missing cwd → handle edge case

**Verification**:
```bash
# Test with invalid API key
CEREBRAS_API_KEY=invalid cargo run -- "test"
# Should show: "Authentication failed. Check your API key in ~/.li/config"

# Test with network timeout (disconnect internet temporarily)
cargo run -- "test"
# Should show: "Network timeout. Check your connection."

# Test with empty input
cargo run -- ""
# Should show: "No input provided. Usage: li \"your task\""
```

---

## Phase 8: Testing

### Milestone 8.1: Unit Tests
**Goal**: Test core logic without API calls.

**Implementation**:
- Config parsing tests
- JSON schema validation tests
- Tool probing mocks
- Exit code logic tests

**Verification**:
```bash
cargo test
# All unit tests should pass
```

### Milestone 8.2: Integration Tests
**Goal**: End-to-end tests with real API.

**Implementation** (`tests/integration_test.rs`):
```rust
#[tokio::test]
async fn test_git_repo_creation() {
    let output = Command::new("cargo")
        .args(&["run", "--", "make a new git repo"])
        .output()
        .expect("Failed to run li");
    
    // Verify plan was generated
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("git init"));
}

#[tokio::test]
async fn test_classifier_terminal() {
    let output = Command::new("cargo")
        .args(&["run", "--", "--classify-only", "ls -la"])
        .output()
        .expect("Failed to run li");
    
    assert_eq!(output.status.code(), Some(100));
}

#[tokio::test]
async fn test_classifier_nl() {
    let output = Command::new("cargo")
        .args(&["run", "--", "--classify-only", "show files"])
        .output()
        .expect("Failed to run li");
    
    assert_eq!(output.status.code(), Some(0));
}
```

**Verification**:
```bash
# Run integration tests (requires API key)
CEREBRAS_API_KEY=your-key cargo test --test integration_test
# All integration tests should pass
```

---

## Phase 9: Documentation & Packaging

### Milestone 9.1: User Documentation
**Goal**: README with installation and usage instructions.

**Create**:
- `README.md` with:
  - Quick start guide
  - Installation instructions
  - Configuration examples
  - Usage examples
  - Troubleshooting

**Verification**:
- Follow README instructions on a clean machine
- Verify all examples work

### Milestone 9.2: Build & Release
**Goal**: Cross-platform binaries.

**Implementation**:
- Build for macOS (arm64, x86_64)
- Build for Linux (x86_64)
- Create GitHub release with binaries
- Generate checksums

**Verification**:
```bash
# Build release binaries
cargo build --release

# Test binary
./target/release/li --version
./target/release/li "test command"
```

### Milestone 9.3: Homebrew Formula
**Goal**: Easy installation via Homebrew.

**Implementation**:
- Create Homebrew tap repository
- Write formula (per PRD section 12.2)
- Test installation

**Verification**:
```bash
# From a clean machine
brew tap yourorg/li
brew install li
li --version
li "make a test directory"
```

---

## Success Criteria

✓ `li "plain english"` generates and executes safe shell plans  
✓ Classifier correctly distinguishes NL from shell commands  
✓ Plans include dry-run probes before destructive operations  
✓ Commands execute in current shell with inherited context  
✓ Hook installation allows natural language at shell prompt  
✓ Config file at `~/.li/config` works correctly  
✓ All tests pass (unit + integration)  
✓ Homebrew installation works end-to-end  

---

## Timeline Estimate

- **Phase 1-2** (Setup + API client): 1-2 days
- **Phase 3** (Classifier): 1 day
- **Phase 4** (Planner): 2-3 days
- **Phase 5** (Executor): 1-2 days
- **Phase 6** (Hook): 2-3 days
- **Phase 7** (Polish): 1 day
- **Phase 8** (Testing): 2 days
- **Phase 9** (Docs + Package): 1-2 days

**Total**: ~2 weeks for v1.0
