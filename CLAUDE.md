# JARVIS - Multi-Agent Mission Control

Tauri 2 desktop app (Rust backend + Svelte 5 frontend) for orchestrating AI agents across multiple machines via Tailscale + SSH.

## Quick Start

```bash
# Dev mode (frontend + backend hot reload)
cd src-tauri && cargo tauri dev

# Build
cargo tauri build

# Check only
cd src-tauri && cargo check
npx svelte-check --threshold error
```

## Architecture

```
src-tauri/src/          # Rust backend (Tauri commands)
  lib.rs                # App entry, plugin/state setup, command registration
  config.rs             # TOML config (~/.config/jarvis/config.toml)
  machines.rs           # MachineRegistry, health checks, stats
  session.rs            # Session monitor, JSONL activity parser
  tasks.rs              # Task dispatch (local/SSH), execute actions
  planning.rs           # Ping-pong planning mode, repo status, branch switching
  pipelines.rs          # Multi-step pipeline orchestration
  github.rs             # GitHub CLI integration (PRs, checks, compare)
  visibility.rs         # Timeline/analytics from JSONL files
  types.rs              # All shared Rust types

src/lib/                # Svelte 5 frontend
  api.ts                # Tauri invoke wrappers
  types.ts              # TypeScript interfaces
  components/
    Header.svelte       # Top bar: session info, branch chips, dropdowns
    AgentPanel.svelte   # Real-time agent activity feed
    CommandBar.svelte   # Bottom command input
    tabs/               # Feature tabs (Settings, Machines, Tasks, etc.)
  stores/
    session.ts          # Session + activity stores (SSE/polling)
    tasks.ts            # Task store with event listeners
    machines.ts         # Machine store
    planning.ts         # Planning state store
    notifications.ts    # Toast notifications
```

## Config System

All machine config lives in `~/.config/jarvis/config.toml` (never hardcoded).

## Key Patterns

- **MachineRegistry**: Central state (`app.state::<MachineRegistry>()`). All modules read config from here.
- **JSONL path derivation**: `session::repo_path_to_jsonl_dir()` converts repo paths to Claude's `~/.claude/projects/` directory format.
- **Local vs Remote**: `machine.host == "local"` runs commands directly; otherwise via SSH.
- **Svelte 5 runes**: Use `$state`, `$derived`, `$effect`, `$props` (NOT `$:` or `export let`).
- **Tauri errors**: Come as plain strings, not Error objects.

## Do NOT

- Hardcode machine names, paths, or SSH hosts — use config
- Use `overflow: hidden` on containers with dropdowns/tooltips
- Skip `cargo check` after Rust changes
