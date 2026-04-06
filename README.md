# JARVIS - Multi-Agent Mission Control

A desktop application for orchestrating AI coding agents across multiple machines via SSH. Built with **Tauri 2** (Rust backend + Svelte 5 frontend).

![License](https://img.shields.io/badge/license-MIT-blue.svg)

## What is JARVIS?

JARVIS gives you a single control panel to manage Claude Code sessions running on different machines in your network. Monitor agent activity in real-time, dispatch tasks, manage git workflows, and coordinate multi-machine development from one place.

### Key Features

- **Multi-machine orchestration** — Manage local and remote machines via SSH/Tailscale
- **Real-time agent monitoring** — Live activity feed from Claude Code JSONL sessions
- **Task dispatch** — Send tasks to any machine, track progress and results
- **Git integration** — Branch management, commit tracking, PR workflows via GitHub CLI
- **Pipeline orchestration** — Define and run multi-step pipelines across machines
- **Planning mode** — Collaborative ping-pong planning with AI agents
- **WhatsApp bridge** — Optional integration for mobile notifications and commands
- **Voice mode** — Optional LiveKit-based voice interaction
- **Activity timeline** — Analytics and visibility into agent work patterns
- **System tray** — Runs in background with quick access

## Prerequisites

- [Rust](https://rustup.rs/) (1.77+)
- [Node.js](https://nodejs.org/) (20+)
- [Tauri CLI](https://tauri.app/start/prerequisites/)
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) installed on target machines

## Quick Start

```bash
# Clone the repo
git clone https://github.com/emawritz/share-jarvis.git
cd share-jarvis

# Install frontend dependencies
npm install

# Run in development mode (hot reload for both frontend and backend)
cd src-tauri && cargo tauri dev
```

On first launch, JARVIS creates a default config at `~/.config/jarvis/config.toml`. Edit this file to add your machines:

```toml
[session]
id = ""
rama = ""
objetivo = ""

[[machines]]
id = "main"
name = "MAIN"
host = "local"          # "local" = this machine
os = "macos"
role = "orchestrator"
enabled = true
tags = ["local"]

[[machines.repos]]
name = "my-project"
path = "~/projects/my-project"
github = "user/my-project"

[[machines]]
id = "worker"
name = "WORKER"
host = "worker-ssh"     # SSH alias from ~/.ssh/config
os = "linux"
enabled = true
gpu = "RTX 3070"

[[machines.repos]]
name = "my-frontend"
path = "~/projects/my-frontend"
github = "user/my-frontend"
```

## Build

```bash
# Production build
cd src-tauri && cargo tauri build

# Type checking
cargo check                              # Rust
npx svelte-check --threshold error       # Svelte/TypeScript
```

## Architecture

```
src-tauri/src/     Rust backend (Tauri commands)
  lib.rs           App setup, plugin registration, command routing
  config.rs        TOML config management (~/.config/jarvis/config.toml)
  machines.rs      Machine registry, health checks, system stats
  session.rs       Session monitor, JSONL activity parsing
  tasks.rs         Task dispatch (local/SSH), execution engine
  planning.rs      Ping-pong planning mode, branch management
  pipelines.rs     Multi-step pipeline orchestration
  github.rs        GitHub CLI integration (PRs, checks)
  visibility.rs    Timeline analytics from JSONL files
  whatsapp.rs      WhatsApp bridge integration (optional)
  db.rs            SQLite knowledge base
  types.rs         Shared Rust types

src/lib/           Svelte 5 frontend
  components/      UI components (Header, CommandBar, tabs/)
  stores/          Reactive stores (session, tasks, machines, planning)
  api.ts           Tauri invoke wrappers
  types.ts         TypeScript interfaces
```

## Optional Services

JARVIS can integrate with these optional services (not included in this repo):

| Service | Port | Purpose |
|---------|------|---------|
| wa-bridge | 3142 | WhatsApp notifications and commands |
| Voice Agent | 3144 | LiveKit-based voice interaction |

These are independent Node.js/Python services. JARVIS works fully without them.

## Tech Stack

- **Backend:** Rust, Tauri 2, Tokio, Axum, rusqlite
- **Frontend:** Svelte 5, TypeScript, Vite
- **Integrations:** GitHub CLI, SSH, Tailscale

## License

[MIT](LICENSE) - Emanuel Cejas
