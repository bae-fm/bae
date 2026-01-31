# bae Project Guidelines

## Project Overview

This is "bae" - always stylized in lowercase. Never "Bae" or "BAE" in user-visible strings (UI, docs, errors, titles, file names, URLs). Code identifiers follow language conventions.

## Core Philosophy

**YAGNI** - Don't leave dead code around. Remove unused code. Run clippy before commits.

**No backwards compatibility concerns** - This project is new and quickly developing.

**Don't bail out** - When working on a fix, keep going until complete. Don't switch approaches ("let's just leave the warning") without asking first. Work through obstacles.

**Only fix issues you introduce** - Don't fix pre-existing linter errors, warnings, or test failures unrelated to current work.

**All work happens in worktrees** - Never work directly in the main checkout. For any new task, create a worktree: `git worktree add .worktrees/<branch-name> -b <branch-name>`, cd into it, and do all work there. The main checkout stays on `main`. Remember to run `npm install` in the worktree (see Worktree setup section below).

## Git Conventions

- Always use `git --no-pager` (before the command, e.g., `git --no-pager diff`)
- Don't use `git add -A` - add files individually or targeted
- Never use `--no-verify` - if hooks fail, fix the issue
- Commit messages: brief, focus on why not what, no marketing language ("improves user experience"), just state what happened

## Rust/Dioxus Patterns

### Dependency Injection
Initialize dependencies at the top (starting in main.rs), pass them down. No singletons.

### Reactive Components
- Components accept `ReadSignal` props for reactivity
- Pass signals down, read at leaf level - don't read in parent and pass values
- AppState is the single source of truth (Dioxus Store)
- Import store extension traits when using lens methods

```rust
// Good: pass signal down, read at leaf
fn Parent(state: ReadSignal<AppState>) {
    rsx! { Child { state } }
}

fn Child(state: ReadSignal<AppState>) {
    let value = state.read().some_field;
    rsx! { div { "{value}" } }
}

// Bad: parent subscribes to all changes
fn Parent(state: ReadSignal<AppState>) {
    let value = state.read().some_field;
    rsx! { Child { value } }
}
```

### Component Props
- Avoid optional props and defaults - consult user first if truly necessary
- Callback props must be non-optional (callers pass no-ops if unused)
- Don't use `#[props(default)]` on enum-typed props

### Enums
- Don't derive `Default` on enums or use `#[default]` attributes
- Put associated data directly in variants, not in separate fields

```rust
// Bad
enum Mode { Created, Loading, Ready }
struct State { mode: Mode, loading_id: Option<String> }

// Good
enum Mode { Created, Loading(String), Ready }
```

### Don't Create Duplicate Types
Don't create `FooInfo` variants of `Foo` for display - use the full type, ignore extra fields.

## Testing

- **TDD**: When debugging, replicate the issue in a test first (unless explicitly told otherwise)
- **Test production code**: Don't create test-only wrappers that duplicate production logic

## Code Style

### Log Statements
Add blank lines before/after log statements when surrounded by substantial code. Skip spacing when log is first in block or follows a comment.

### Icons
- No emojis as icons - use proper SVG icons
- No music note icons (Lucide's music, music-2, etc.) - use specific icons instead:
  - Missing album art: `ImageIcon`
  - Track list/multiple files: `RowsIcon`
  - Single disc/CUE+FLAC: `DiscIcon`

## Worktree / Fresh Checkout Setup

The Rust build scripts in `bae-desktop` and `bae-mocks` shell out to `node_modules/.bin/tailwindcss` to generate CSS. The generated CSS files are gitignored, and `node_modules/` doesn't carry over to worktrees. The build will panic if tailwind isn't installed.

After creating a worktree or fresh clone, run:
```sh
git submodule update --init
(cd bae-desktop && npm install)
(cd bae-mocks && npm install)
```

## Dependencies

When adding a new dependency (crate, npm package), always look up the latest version first.
