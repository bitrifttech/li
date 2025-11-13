# Rust Syntax Guide for Python/C++ Developers

## Basic Rust Concepts Compared to Python/C++

### 1. Module System and Imports
**Rust:**
```rust
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use tokio::process::Command as TokioCommand;
use crate::planner::Plan;
```

**Python equivalent:**
```python
from anyhow import Result, anyhow
from std.collections import HashMap
# (Note: these don't exist in Python, this is conceptual)
```

**C++ equivalent:**
```cpp
#include <anyhow/result.hpp>
#include <unordered_map>
```

**Key points:**
- `use` is like `import` in Python or `#include` in C++
- `crate::` refers to the current project root (like relative imports)
- `{}` destructures imports (get only what you need)

### 2. Struct Definitions (like classes without methods)
**Rust:**
```rust
pub struct CommandValidator {
    cache: HashMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    pub missing_commands: Vec<MissingCommand>,
    pub plan_can_continue: bool,
}
```

**Python equivalent:**
```python
class CommandValidator:
    def __init__(self):
        self.cache = {}

@dataclass
class ValidationResult:
    missing_commands: List[MissingCommand]
    plan_can_continue: bool
```

**C++ equivalent:**
```cpp
class CommandValidator {
    std::unordered_map<std::string, bool> cache;
public:
    // methods...
};

struct ValidationResult {
    std::vector<MissingCommand> missing_commands;
    bool plan_can_continue;
};
```

**Key points:**
- `pub` makes fields public (accessible from outside)
- `#[derive(...)]` auto-implements traits (like interfaces/mixins)
- `struct` is like a lightweight class without inheritance

### 3. Implementation Blocks (methods for structs)
**Rust:**
```rust
impl CommandValidator {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
    
    pub async fn command_exists(&mut self, cmd: &str) -> bool {
        // method implementation
    }
}
```

**Python equivalent:**
```python
class CommandValidator:
    def __new__(cls):
        return cls()
    
    async def command_exists(self, cmd: str) -> bool:
        # method implementation
```

**C++ equivalent:**
```cpp
CommandValidator::CommandValidator() {
    cache = std::unordered_map<std::string, bool>();
}

bool CommandValidator::command_exists(const std::string& cmd) {
    // method implementation
}
```

**Key points:**
- `impl` defines methods for a struct
- `Self` refers to the type being implemented
- `&mut self` is like `self` in Python (mutable reference)
- `&str` is a string slice (borrowed string reference)

### 4. Error Handling
**Rust:**
```rust
pub async fn validate_plan(&mut self, plan: &Plan) -> Result<ValidationResult> {
    // implementation
}

let command_name = Self::extract_command(cmd_line)
    .ok_or_else(|| anyhow!("Could not extract command from: {}", cmd_line))?;
```

**Python equivalent:**
```python
async def validate_plan(self, plan: Plan) -> ValidationResult:
    # implementation

try:
    command_name = self.extract_command(cmd_line)
except:
    raise ValueError(f"Could not extract command from: {cmd_line}")
```

**C++ equivalent:**
```cpp
std::expected<ValidationResult, std::error_code> validate_plan(Plan& plan) {
    // implementation
}

auto command_name = extract_command(cmd_line);
if (!command_name) {
    return std::unexpected(std::format("Could not extract command from: {}", cmd_line));
}
```

**Key points:**
- `Result<T>` is like Python's exceptions or C++'s std::expected
- `?` operator propagates errors (like try-catch chain)
- `anyhow!` creates error values

### 5. Option Handling (null safety)
**Rust:**
```rust
pub fn extract_command(cmd: &str) -> Option<String> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    // ... rest of function
    Some(cleaned_cmd.to_string())
}

let first_token = primary_cmd.split_whitespace().next()?;
```

**Python equivalent:**
```python
def extract_command(cmd: str) -> Optional[str]:
    trimmed = cmd.strip()
    if not trimmed:
        return None
    # ... rest of function
    return cleaned_cmd

first_token = next(primary_cmd.split(), None)
if first_token is None:
    return None
```

**C++ equivalent:**
```cpp
std::optional<std::string> extract_command(const std::string& cmd) {
    auto trimmed = trim(cmd);
    if (trimmed.empty()) {
        return std::nullopt;
    }
    // ... rest of function
    return cleaned_cmd;
}

auto first_token = get_first_token(primary_cmd);
if (!first_token) {
    return std::nullopt;
}
```

**Key points:**
- `Option<T>` is either `Some(value)` or `None`
- `?` on Option returns None if None, unwraps if Some
- Eliminates null pointer exceptions

### 6. Pattern Matching
**Rust:**
```rust
match result {
    Ok(output) => output.status.success(),
    Err(_) => {
        // fallback logic
        fallback_result.map(|output| output.status.success()).unwrap_or(false)
    }
}
```

**Python equivalent:**
```python
if result.is_ok():
    return result.ok().status.success()
else:
    # fallback logic
    return fallback_result.map(lambda output: output.status.success()).get_or_false()
```

**C++ equivalent:**
```cpp
if (result.has_value()) {
    return result.value().status.success();
} else {
    // fallback logic
    return fallback_result.transform([](auto& output) { return output.status.success(); }).value_or(false);
}
```

**Key points:**
- `match` is exhaustive (must handle all cases)
- `Ok` and `Err` are the two variants of `Result`
- `_` is a wildcard pattern (catch-all)

## Analysis of validator/mod.rs Structures

### 1. CommandValidator struct
**Purpose:** Main validator class that checks system command availability
**Fields:**
- `cache: HashMap<String, bool>` - Caches command existence results for performance

**Methods:**
- [`new()`](src/validator/mod.rs:27) - Constructor, creates new validator with empty cache
- [`extract_command()`](src/validator/mod.rs:34) - Static method that extracts primary command from complex command lines
- [`command_exists()`](src/validator/mod.rs:73) - Async method to check if command exists (with caching)
- [`validate_plan()`](src/validator/mod.rs:132) - Validates all commands in execution plan
- [`check_single_command()`](src/validator/mod.rs:173) - Validates individual command
- [`get_available_tools()`](src/validator/mod.rs:181) - Returns list of available common tools

### 2. ValidationResult struct
**Purpose:** Contains results of command validation
**Fields:**
- `missing_commands: Vec<MissingCommand>` - List of commands that weren't found
- `plan_can_continue: bool` - Whether execution can proceed

### 3. MissingCommand struct
**Purpose:** Information about a missing command
**Fields:**
- `command: String` - The actual command name that's missing
- `failed_command_line: String` - Full command line that failed
- `plan_step: usize` - Index in the execution plan
- `is_dry_run: bool` - Whether this was a dry-run or execute command

## Key Rust idioms used:

1. **Builder pattern in constructors:** `Self { cache: HashMap::new() }`
2. **Error propagation:** `?` operator for Result and Option
3. **Borrowing:** `&str`, `&mut self` for memory safety
4. **Async/await:** `async fn`, `.await` for non-blocking operations
5. **Traits for behavior:** `#[derive(Debug, Clone, PartialEq)]` for common behaviors
6. **Pattern matching:** `match`, `if let` for handling different cases
7. **Method chaining:** `.map().unwrap_or()` for functional-style operations
8. **String handling:** `.to_string()`, `.trim()`, string slices for efficient string operations

The code follows Rust's ownership and borrowing rules to ensure memory safety without garbage collection, while providing functionality similar to what you'd expect from Python/C++ classes but with compile-time guarantees.