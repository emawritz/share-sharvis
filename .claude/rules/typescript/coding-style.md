> This file extends [common/coding-style.md](../common/coding-style.md) with TypeScript-specific content.

# TypeScript Coding Style

## Type Safety

- Use `interface` for object shapes, `type` for unions/intersections
- NEVER use `any` — use `unknown` and narrow with type guards
- Use `as const` for literal types
- Prefer `readonly` arrays and properties

```typescript
// WRONG
const config: any = getConfig();

// CORRECT
const config: unknown = getConfig();
if (isAppConfig(config)) {
  // config is now typed
}
```

## React/Svelte Props

```typescript
// Define props with interfaces
interface ButtonProps {
  readonly label: string;
  readonly variant?: 'primary' | 'secondary';
  readonly onClick: () => void;
}
```

## Validation with Zod

Use Zod for runtime validation at system boundaries:

```typescript
import { z } from 'zod';

const UserSchema = z.object({
  name: z.string().min(1),
  email: z.string().email(),
  role: z.enum(['admin', 'user']),
});

type User = z.infer<typeof UserSchema>;
```

## Immutability

```typescript
// WRONG: mutation
arr.push(item);
obj.key = value;

// CORRECT: new references
const newArr = [...arr, item];
const newObj = { ...obj, key: value };
```

## Null Handling

- Use optional chaining (`?.`) and nullish coalescing (`??`)
- Prefer `undefined` over `null` for optional values
- Use strict null checks (`strictNullChecks: true`)
