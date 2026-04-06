> This file extends [common/patterns.md](../common/patterns.md) with TypeScript-specific content.

# TypeScript Patterns

## API Response Envelope

```typescript
interface ApiResponse<T> {
  readonly success: boolean;
  readonly data: T | null;
  readonly error: string | null;
  readonly meta?: {
    readonly total: number;
    readonly page: number;
    readonly limit: number;
  };
}
```

## Debounce Hook (Svelte 5)

```typescript
function useDebounce<T>(value: T, delay: number): T {
  let debounced = $state(value);
  let timeout: ReturnType<typeof setTimeout>;

  $effect(() => {
    timeout = setTimeout(() => {
      debounced = value;
    }, delay);
    return () => clearTimeout(timeout);
  });

  return debounced;
}
```

## Repository Pattern

```typescript
interface Repository<T> {
  findAll(): Promise<T[]>;
  findById(id: string): Promise<T | null>;
  create(data: Omit<T, 'id'>): Promise<T>;
  update(id: string, data: Partial<T>): Promise<T>;
  delete(id: string): Promise<void>;
}
```

## Error Handling Pattern

```typescript
class AppError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly statusCode: number = 500,
  ) {
    super(message);
    this.name = 'AppError';
  }
}

// Usage
throw new AppError('User not found', 'USER_NOT_FOUND', 404);
```
