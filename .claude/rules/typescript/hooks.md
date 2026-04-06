> This file extends [common/hooks.md](../common/hooks.md) with TypeScript-specific content.

# TypeScript Hooks

## Recommended PostToolUse Hooks

### Auto-format with Prettier

```json
{
  "event": "PostToolUse",
  "matcher": { "tool": "Write" },
  "command": "npx prettier --write $FILE_PATH"
}
```

### Type Check after edits

```json
{
  "event": "PostToolUse",
  "matcher": { "tool": "Write" },
  "command": "npx tsc --noEmit"
}
```

### Warn on console.log

```json
{
  "event": "PostToolUse",
  "matcher": { "tool": "Write" },
  "command": "grep -n 'console.log' $FILE_PATH && echo 'WARNING: console.log found' || true"
}
```

## Svelte-Specific

### Svelte Check after component edits

```json
{
  "event": "PostToolUse",
  "matcher": { "tool": "Write", "glob": "**/*.svelte" },
  "command": "npx svelte-check --threshold error"
}
```
