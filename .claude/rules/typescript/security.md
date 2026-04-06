> This file extends [common/security.md](../common/security.md) with TypeScript-specific content.

# TypeScript Security

## Secret Management

```typescript
// WRONG
const API_KEY = 'sk-1234567890';

// CORRECT
const API_KEY = process.env.API_KEY;
if (!API_KEY) throw new Error('API_KEY is required');
```

## Input Sanitization

- Use DOMPurify for HTML content
- Use parameterized queries for database access
- Validate all API inputs with Zod schemas

```typescript
import DOMPurify from 'dompurify';

const clean = DOMPurify.sanitize(userInput);
```

## Environment Variables

- Use `.env.example` to document required variables (no real values)
- Validate env vars at startup with Zod
- Never log environment variable values

```typescript
const EnvSchema = z.object({
  DATABASE_URL: z.string().url(),
  API_KEY: z.string().min(1),
  NODE_ENV: z.enum(['development', 'production', 'test']),
});

const env = EnvSchema.parse(process.env);
```
