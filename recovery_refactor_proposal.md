# Recovery Module Refactor Proposal

## Current Structure Analysis

### External API Methods (called from outside the module)
- [`RecoveryEngine::new()`](src/recovery/mod.rs:111) - Constructor
- [`RecoveryEngine::set_available_tools()`](src/recovery/mod.rs:121) - Setup system context
- [`RecoveryEngine::generate_recovery_options()`](src/recovery/mod.rs:128) - Main entry point
- [`RecoveryEngine::should_attempt_recovery()`](src/recovery/mod.rs:163) - Configuration check
- [`RecoveryEngine::present_recovery_menu()`](src/recovery/mod.rs:503) - User interaction
- [`RecoveryEngine::execute_recovery()`](src/recovery/mod.rs:664) - Execute chosen strategy

### Internal Implementation Methods
- AI interaction methods
- Prompt building methods  
- Fallback generation methods
- Utility methods for parsing and processing

### Data Structures
- RecoveryEngine (main struct)
- RecoveryOptions, RecoveryChoice, RecoveryResult (enums/structs)
- CommandAlternative, InstallationInstruction
- RecoveryContext, RecoveryResponse

## Proposed New File Structure

```
src/recovery/
├── mod.rs              # Public API and main RecoveryEngine struct
├── types.rs            # All data structures and enums
├── ai.rs               # AI interaction and prompt building
├── ui.rs               # User interface and menu presentation
├── utils.rs            # Utility methods and fallback generation
└── tests.rs            # Tests (existing)
```

## File Responsibilities

### 1. `mod.rs` - Public API
**Purpose:** Contains only the external interface that other modules call
**Contents:**
- RecoveryEngine struct definition
- Public API methods (new, set_available_tools, generate_recovery_options, etc.)
- Module declarations and re-exports
- Basic plumbing to internal modules

**External Methods:**
- `new()` - Initialize engine
- `set_available_tools()` - Setup system context
- `generate_recovery_options()` - Main recovery logic orchestration
- `should_attempt_recovery()` - Configuration validation
- `present_recovery_menu()` - Delegate to ui module
- `execute_recovery()` - Execute recovery strategy

### 2. `types.rs` - Data Structures
**Purpose:** All data types, enums, and their implementations
**Contents:**
- RecoveryEngine struct (fields only, no methods)
- RecoveryOptions, RecoveryChoice, RecoveryResult enums
- CommandAlternative, InstallationInstruction structs
- RecoveryContext, RecoveryResponse structs
- Display implementations for types
- RecoveryOptions::skip_only() method

### 3. `ai.rs` - AI Interaction
**Purpose:** All AI-related functionality
**Contents:**
- `extract_json_from_markdown()` - Parse AI responses
- `build_recovery_prompt()` - Create alternative-focused prompts
- `build_installation_prompt()` - Create installation-focused prompts
- `generate_alternatives_first()` - AI alternative generation
- `generate_installation_first()` - AI installation generation
- `convert_response_to_options()` - Parse AI JSON responses

### 4. `ui.rs` - User Interface
**Purpose:** All user interaction and presentation logic
**Contents:**
- `present_recovery_menu()` - Main menu presentation
- `parse_user_choice()` - Input validation and parsing
- Terminal formatting and colored output
- User input handling

### 5. `utils.rs` - Utilities and Fallbacks
**Purpose:** Helper methods and fallback logic
**Contents:**
- `generate_fallback_alternatives()` - Hardcoded command alternatives
- `generate_fallback_instructions()` - Platform-specific installation commands
- Common utility functions
- Platform detection helpers

## Benefits of This Structure

### 1. Clear Separation of Concerns
- Each file has a single, well-defined responsibility
- Easier to understand and maintain
- Reduces cognitive load when working on specific functionality

### 2. Better Testability
- Individual modules can be tested in isolation
- AI functionality can be mocked and tested separately
- UI logic can be tested without touching other concerns

### 3. Improved Organization
- External API is clear and focused in mod.rs
- Data types are centralized for easy reference
- Specialized functionality is grouped logically

### 4. Easier Extension
- Adding new AI providers goes in ai.rs
- New UI improvements go in ui.rs
- Additional fallbacks go in utils.rs

### 5. Reduced Compilation Times
- Changes to UI don't require recompiling AI logic
- Type changes are isolated to one file
- Better incremental compilation

## Migration Strategy

### Phase 1: Create New Files
1. Create types.rs with all data structures
2. Create utils.rs with utility methods
3. Create ai.rs with AI interaction logic
4. Create ui.rs with user interface code

### Phase 2: Update mod.rs
1. Refactor RecoveryEngine to use internal modules
2. Keep only public API methods
3. Add proper module declarations and use statements

### Phase 3: Update Tests
1. Ensure imports work correctly
2. Run existing tests to verify functionality
3. Add module-specific tests if needed

### Phase 4: Verification
1. Run all tests
2. Verify external API unchanged
3. Check that all functionality is preserved

## Backward Compatibility

- External API remains exactly the same
- All public methods maintain identical signatures
- No changes to how other modules interact with recovery
- Tests continue to work without modification

## File Size Estimates

- **mod.rs**: ~100 lines (down from 842)
- **types.rs**: ~150 lines
- **ai.rs**: ~200 lines  
- **ui.rs**: ~180 lines
- **utils.rs**: ~120 lines
- **tests.rs**: ~79 lines (unchanged)

This makes each file much more manageable and focused on specific concerns.