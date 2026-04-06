<p align="center">
  <img src="https://img.icons8.com/color/96/artificial-intelligence.png" alt="JARVIS Logo" width="96">
</p>

<h1 align="center">JARVIS</h1>

<p align="center">
  <strong>Multi-Agent Mission Control for AI Coding Agents</strong>
</p>

<p align="center">
  <a href="https://github.com/emawritz/share-sharvis/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/tauri-2.0-blue?logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/rust-1.77+-orange?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/svelte-5-ff3e00?logo=svelte&logoColor=white" alt="Svelte 5">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey" alt="Platform">
</p>

<p align="center">
  A single control panel to orchestrate <a href="https://docs.anthropic.com/en/docs/claude-code">Claude Code</a> sessions running across multiple machines in your network.<br>
  Monitor agents in real-time, dispatch tasks, manage git workflows, and coordinate multi-machine development — all from one place.
</p>

---

## Why JARVIS?

Managing AI coding agents across multiple machines is painful. You SSH into one box, check a session, switch to another, lose context, forget what's running where. JARVIS solves this by giving you **one dashboard** to see everything.

- **No cloud required** — runs entirely on your local network via SSH/Tailscale
- **Real-time visibility** — live activity feed parsed from Claude Code JSONL sessions
- **Multi-machine** — manage local and remote machines from a single window
- **Native desktop app** — built with Tauri 2, not a browser tab

---

## ✨ Features

| Feature | Description |
|---------|-------------|
| 🖥️ **Machine Registry** | Add any machine (local or remote via SSH). Health checks, system stats, GPU info |
| 📡 **Live Agent Feed** | Real-time activity stream parsed from Claude Code session files |
| 📋 **Task Dispatch** | Send tasks to any machine, track execution and results |
| 🔀 **Git Integration** | Branch management, commit tracking, PR workflows via GitHub CLI |
| 🔗 **Pipelines** | Define multi-step pipelines that run across machines |
| 🤝 **Planning Mode** | Collaborative ping-pong planning between you and AI agents |
| 📊 **Activity Timeline** | Analytics and visibility into agent work patterns |
| 🔔 **System Tray** | Runs in background with quick access |

<details>
<summary><strong>Optional integrations</strong></summary>

| Service | Port | Purpose |
|---------|------|---------|
| WhatsApp Bridge | 3142 | Mobile notifications and commands |
| Voice Agent | 3144 | LiveKit-based voice interaction |

These are independent services — JARVIS works fully without them.

</details>

---

## 🚀 Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 20+
- [Tauri CLI](https://tauri.app/start/prerequisites/)
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) installed on target machines

### Install & Run

```bash
git clone https://github.com/emawritz/share-sharvis.git
cd share-sharvis
npm install
cd src-tauri && cargo tauri dev
```

On first launch, JARVIS creates a default config at `~/.config/jarvis/config.toml`.

<details>
<summary><strong>Example config</strong></summary>

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

</details>

---

## 🏗️ Architecture

```
src-tauri/src/          Rust backend (Tauri commands)
├── lib.rs              App setup, plugin registration, command routing
├── config.rs           TOML config management
├── machines.rs         Machine registry, health checks, system stats
├── session.rs          Session monitor, JSONL activity parsing
├── tasks.rs            Task dispatch (local/SSH), execution engine
├── planning.rs         Ping-pong planning mode, branch management
├── pipelines.rs        Multi-step pipeline orchestration
├── github.rs           GitHub CLI integration (PRs, checks)
├── visibility.rs       Timeline analytics from JSONL files
└── types.rs            Shared Rust types

src/lib/                Svelte 5 frontend
├── components/         UI components (Header, CommandBar, tabs/)
├── stores/             Reactive stores (session, tasks, machines, planning)
├── api.ts              Tauri invoke wrappers
└── types.ts            TypeScript interfaces
```

### How It Works

```mermaid
flowchart TB
    subgraph SETUP["🚀 First Launch"]
        W1[Setup Wizard] --> W2[Detect Local Machine]
        W2 --> W3[Add Remote Machines]
        W3 --> W4[Test SSH Connections]
        W4 --> W5[Save config.toml]
    end

    subgraph CORE["⚙️ Runtime"]
        CFG[~/.config/jarvis/config.toml] -->|load| REG[Machine Registry]
        REG -->|spawn| HC[Health Checks]
        REG -->|spawn| SM[Session Monitor]
        SM -->|parse| JSONL[Claude JSONL Files]
    end

    subgraph MACHINES["🖥️ Machines"]
        LOCAL[Local Machine\nshell exec]
        REMOTE1[Remote Machine 1\nSSH + Tailscale]
        REMOTE2[Remote Machine 2\nSSH + Tailscale]
    end

    subgraph UI["🎨 Dashboard · Svelte 5"]
        TABS[Tabs: Tasks · Machines · Commits · GitHub · Pipelines · Timeline]
        CMD[Command Bar]
        AGENT[Agent Activity Feed]
    end

    W5 -->|generates| CFG
    HC -->|status| UI
    SM -->|activity stream| AGENT
    CMD -->|dispatch task| REG
    REG -->|host = local| LOCAL
    REG -->|host = ssh-alias| REMOTE1
    REG -->|host = ssh-alias| REMOTE2
    LOCAL -->|stdout/stderr| UI
    REMOTE1 -->|stdout/stderr| UI
    REMOTE2 -->|stdout/stderr| UI
```

**Flow summary:**

1. **Setup Wizard** auto-detects your local machine and lets you add remotes via SSH
2. **Config** is saved to `~/.config/jarvis/config.toml` — fully dynamic, never hardcoded
3. **Machine Registry** loads config, spawns health checks and session monitors
4. **Session Monitor** parses Claude Code JSONL files for real-time agent activity
5. **Task Dispatch** routes commands to local shell or remote machines via SSH/Tailscale
6. **Dashboard** surfaces everything in a unified Svelte 5 UI with live updates

---

## 🛠️ Tech Stack

- **Backend:** Rust, Tauri 2, Tokio, Axum, rusqlite
- **Frontend:** Svelte 5, TypeScript, Vite
- **Integrations:** GitHub CLI, SSH, Tailscale

---

## 🏗️ Build

```bash
# Production build
cd src-tauri && cargo tauri build

# Type checking
cargo check                              # Rust
npx svelte-check --threshold error       # Svelte/TypeScript
```

---

## 🤝 Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## 📄 License

[MIT](LICENSE) — Emanuel Cejas
