```markdown
# variete-saveurs-mobile Development Patterns

> Auto-generated skill from repository analysis

## Overview
This skill covers the core development patterns and conventions used in the `variete-saveurs-mobile` Rust codebase. It documents file organization, code style, commit conventions, and testing patterns, providing practical examples and suggested commands for common workflows. This guide is designed to help contributors quickly align with the project's practices.

## Coding Conventions

### File Naming
- **Style:** camelCase
- **Example:**  
  ```
  userProfile.rs
  orderManager.rs
  ```

### Imports
- **Style:** Relative imports
- **Example:**
  ```rust
  mod userProfile;
  use crate::orderManager::Order;
  ```

### Exports
- **Style:** Named exports
- **Example:**
  ```rust
  pub struct UserProfile { /* ... */ }
  pub fn create_order() { /* ... */ }
  ```

### Commit Messages
- **Style:** Conventional commits
- **Prefixes:** `feat`, `fix`
- **Average Length:** ~71 characters
- **Example:**
  ```
  feat: add user authentication to orderManager
  fix: correct price calculation in cartService
  ```

## Workflows

### Feature Development
**Trigger:** When adding a new feature  
**Command:** `/feature-development`

1. Create a new branch for the feature.
2. Implement the feature using camelCase file naming and relative imports.
3. Write or update tests in corresponding `*.test.*` files.
4. Commit changes using the `feat:` prefix and a descriptive message.
5. Open a pull request for review.

### Bug Fixing
**Trigger:** When fixing a bug  
**Command:** `/bug-fix`

1. Create a new branch for the bug fix.
2. Locate the relevant module using camelCase file names.
3. Apply the fix, ensuring code style consistency.
4. Update or add tests in `*.test.*` files to cover the fix.
5. Commit changes using the `fix:` prefix and a descriptive message.
6. Open a pull request for review.

## Testing Patterns

- **Test File Pattern:** `*.test.*`
- **Framework:** Unknown (ensure tests are placed in files matching the pattern)
- **Example:**
  ```
  userProfile.test.rs
  orderManager.test.rs
  ```
- **Practice:** Write tests alongside the modules they cover, following the same camelCase naming.

## Commands
| Command              | Purpose                                    |
|----------------------|--------------------------------------------|
| /feature-development | Start a new feature development workflow   |
| /bug-fix             | Start a bug fixing workflow                |
```
