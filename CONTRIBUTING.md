# Contributing to JARVIS

Thanks for your interest in contributing! Here's how to get started.

## Development Setup

1. Fork and clone the repo
2. Install prerequisites: Rust 1.77+, Node.js 20+, Tauri CLI
3. Run `npm install` for frontend dependencies
4. Run `cd src-tauri && cargo tauri dev` to start development

## Making Changes

- Create a feature branch from `main`
- Follow conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`
- Run checks before submitting:
  ```bash
  cd src-tauri && cargo check        # Rust
  npx svelte-check --threshold error # Frontend
  npm test                           # Tests
  ```

## Pull Requests

- Keep PRs focused on a single change
- Include a description of what and why
- Add tests for new functionality

## Code Style

- **Rust:** Standard rustfmt formatting
- **Svelte/TypeScript:** Svelte 5 runes (`$state`, `$derived`, `$effect`)
- **Config:** Never hardcode machine names, paths, or hosts — use the config system

## Reporting Issues

Open an issue with:
- Steps to reproduce
- Expected vs actual behavior
- OS and version info

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
