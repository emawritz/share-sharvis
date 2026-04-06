# Rules

## Structure

Rules are organized into a **common** layer plus **language-specific** directories:

```
rules/
├── common/          # Language-agnostic principles (always loaded)
│   ├── coding-style.md
│   ├── git-workflow.md
│   ├── testing.md
│   ├── performance.md
│   ├── patterns.md
│   ├── hooks.md
│   ├── agents.md
│   ├── security.md
│   ├── code-review.md
│   └── development-workflow.md
├── typescript/      # TypeScript/Svelte specific
│   ├── coding-style.md
│   ├── testing.md
│   ├── security.md
│   ├── hooks.md
│   └── patterns.md
└── README.md
```

- **common/** contains universal principles — no language-specific code examples.
- **Language directories** extend the common rules with framework-specific patterns, tools, and code examples. Each file references its common counterpart.

## Rule Priority

When language-specific rules and common rules conflict, **language-specific rules take precedence** (specific overrides general).

## Adding a New Language

To add support for a new language (e.g., `rust/`, `python/`):

1. Create a `rules/<language>/` directory
2. Add files that extend the common rules
3. Each file should start with:
   ```
   > This file extends [common/xxx.md](../common/xxx.md) with <Language> specific content.
   ```

## Rules vs Agents

- **Rules** define standards, conventions, and checklists that apply broadly (e.g., "80% test coverage", "no hardcoded secrets").
- **Agents** (`.claude/agents/`) provide specialized task execution (e.g., code review, security analysis, TDD enforcement).
