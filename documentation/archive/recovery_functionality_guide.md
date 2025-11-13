# Recovery Module Functionality Guide

## What This File Does

The [`recovery/mod.rs`](src/recovery/mod.rs) file implements an **intelligent recovery system** that provides alternatives and solutions when commands fail during execution. Think of it like a smart "error recovery assistant" that helps users continue working when tools are missing or unavailable.

**Core Concept:** When a command execution fails, instead of just crashing, this module:
1. Analyzes what went wrong (missing command, failed tool, etc.)
2. Consults an AI to suggest alternatives
3. Provides installation instructions for missing tools
4. Presents an interactive menu for the user to choose how to proceed
5. Executes the chosen recovery strategy

## Core Architecture

The module centers around the **RecoveryEngine** struct that orchestrates the recovery process using AI assistance and system knowledge.

### Primary Workflow

1. **Command Failure Detected** → Recovery engine is invoked with missing command info
2. **Context Gathering** → Collects available tools, original goal, and execution context
3. **AI Consultation** → Sends structured prompt to LLM asking for alternatives
4. **Response Processing** → Parses AI response and creates structured recovery options
5. **User Interaction** → Presents interactive menu with choices
6. **Execution** → Executes the chosen recovery strategy
7. **Result Reporting** → Returns outcome to continue or abort execution

## Method Execution Flow

### 1. RecoveryEngine::new(config: &Config) -> Result<Self>
**When called:** At application startup or when recovery system is initialized
**What it does:**
- Creates new RecoveryEngine instance
- Initializes AI client using configuration
- Sets up empty available tools list
- Configures recovery preferences from settings

### 2. set_available_tools(&mut self) -> Result<()>
**When called:** Before generating recovery options to ensure current system context
**What it does:**
- Creates a CommandValidator instance
- Calls validator's [`get_available_tools()`](src/validator/mod.rs:181) method
- Stores the list of available system tools for AI context
- Ensures AI suggestions are based on actual system capabilities

### 3. generate_recovery_options() - Main Recovery Logic
**When called:** When a missing command is detected during plan execution
**What it does:**
- **Input:** Missing command details, original plan, and goal description
- **Configuration check:** Verifies if recovery is enabled and preferred
- **Context preparation:** Ensures available tools information is current
- **Strategy selection:** Based on configuration preference:
  - **AlternativesFirst**: Tries to find replacement commands first
  - **InstallationFirst**: Prioritizes installing the missing command
  - **SkipOnError**: Only allows skipping the failed step
  - **NeverRecover**: Disables recovery entirely

### 4. generate_alternatives_first() - AI-Powered Alternative Finding
**When called:** When configured to prioritize command alternatives
**What it does:**
- **Prompt building:** Creates detailed AI prompt including:
  - Missing command name
  - Original execution goal
  - Failed command line
  - List of available system tools
  - Operating system information
- **AI consultation:** Sends structured prompt to LLM with specific JSON response format requirements
- **Response parsing:** Extracts JSON from AI response (handles markdown code blocks)
- **Fallback logic:** If AI provides no alternatives, generates simple fallback alternatives
- **Returns:** RecoveryOptions with command alternatives and optional installation instructions

### 5. generate_installation_first() - Installation-Focused Recovery
**When called:** When configured to prioritize tool installation
**What it does:**
- **Building installation prompt:** Creates AI prompt focused on:
  - Installation commands for current platform
  - Package manager suggestions (brew, apt, yum, etc.)
  - Alternative approaches if installation isn't possible
- **Platform-specific context:** Includes OS information for targeted installation instructions
- **Response processing:** Similar to alternatives-first but prioritizes installation instructions
- **Fallback instructions:** If AI fails, generates basic installation commands based on OS detection

### 6. present_recovery_menu() - Interactive User Interaction
**When called:** After generating recovery options to get user decision
**What it does:**
- **Professional menu display:** Shows formatted, colored options with:
  - Alternative commands with confidence scores
  - Installation instructions by package manager
  - Skip/Retry/Abort options when appropriate
- **Input collection:** Reads user choice from stdin
- **Choice parsing:** Handles various input formats:
  - Numbers (1, 2, 3...) for listed options
  - Text commands (skip, retry, abort)
  - Installation prefixes (i1, i2, i3...)
- **Input validation:** Ensures choices are valid and within allowed options

### 7. execute_recovery() - Recovery Strategy Execution
**When called:** After user makes a choice from the recovery menu
**What it does:**
- **Alternative execution:** Runs the chosen replacement command via shell
- **Installation execution:** Executes installation commands (with confirmation if not auto-install)
- **Flow control:** Handles skipping steps, aborting plans, or retrying original commands
- **Result reporting:** Returns structured results indicating success/failure

### 8. Supporting Methods

#### build_recovery_prompt() & build_installation_prompt()
**When called:** Internally by the generation methods
**What they do:**
- Create structured, detailed prompts for the AI
- Include all necessary context (missing command, goal, available tools, OS)
- Request specific JSON response format for reliable parsing
- Provide examples and clear instructions to the AI

#### extract_json_from_markdown()
**When called:** When processing AI responses
**What it does:**
- Handles AI responses that wrap JSON in markdown code blocks
- Extracts pure JSON content from ```json or ``` blocks
- Falls back to raw content if no code blocks are found
- Ensures reliable JSON parsing regardless of AI formatting

#### generate_fallback_alternatives() & generate_fallback_instructions()
**When called:** When AI fails to provide useful suggestions
**What they do:**
- Provide basic, hardcoded alternatives for common commands (tar→zip, curl→wget, etc.)
- Generate platform-specific installation commands based on OS detection
- Ensure users always have at least some recovery options even if AI fails

## Data Structures Explained

### RecoveryOptions
Contains all possible recovery strategies for a failure:
- `command_alternatives`: List of replacement commands with confidence scores
- `installation_instructions`: How to install the missing tool
- `can_skip_step`: Whether this step can be safely skipped
- `retry_possible`: If retrying the original command makes sense

### RecoveryChoice (User Selection)
Enum representing what user decided:
- `UseAlternative(index)`: Execute a suggested alternative command
- `InstallCommand(index)`: Run installation for missing tool
- `SkipStep`: Skip this execution step
- `AbortPlan`: Cancel the entire execution plan
- `RetryOriginal`: Try the original command again

### RecoveryResult (Execution Outcome)
Enum representing what happened after recovery:
- `AlternativeSucceeded/Failed`: Results of trying alternative commands
- `InstallationSucceeded/Failed/Cancelled`: Results of installation attempts
- `StepSkipped`: Step was skipped successfully
- `PlanAborted`: Execution was cancelled
- `RetryRequested`: User wants to retry original approach

## Example Usage Flow

```rust
// Command fails during execution
let recovery_engine = RecoveryEngine::new(&config)?;
recovery_engine.set_available_tools().await?;

// Generate recovery options
let options = recovery_engine.generate_recovery_options(
    &missing_command,
    &original_plan,
    "build and deploy application"
).await?;

// Get user choice
let choice = recovery_engine.present_recovery_menu(&options, &missing_command).await?;

// Execute the recovery strategy
let result = recovery_engine.execute_recovery(
    choice,
    &RecoveryContext {
        missing_command,
        original_plan,
        original_goal: "build and deploy application".to_string(),
    }
).await?;

// Handle the result
match result {
    RecoveryResult::AlternativeSucceeded(alt) => {
        // Continue with next step
    }
    RecoveryResult::PlanAborted(reason) => {
        // Stop execution
    }
    // ... other cases
}
```

## Key Design Features

### AI Integration
- Uses LLM to generate intelligent, context-aware alternatives
- Structured prompts ensure reliable JSON responses
- Fallback mechanisms when AI is unavailable

### User Experience
- Clean, colored terminal interface with clear options
- Multiple input formats (numbers, text commands)
- Confidence scores help users choose best alternatives

### Robustness
- Fallback alternatives for common command failures
- Platform-specific installation instructions
- Error handling at every step

### Flexibility
- Configurable recovery preferences (alternatives first, installation first, etc.)
- Auto-install options for automated environments
- Extensible design for new recovery strategies

This recovery system transforms command failures from show-stopping errors into manageable situations with clear paths forward, significantly improving the user experience when working with automated command execution.