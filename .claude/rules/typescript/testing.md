> This file extends [common/testing.md](../common/testing.md) with TypeScript-specific content.

# TypeScript Testing

## Frameworks

- **Unit/Integration**: Vitest (preferred) or Jest
- **E2E**: Playwright (preferred) or Cypress
- **Component**: Testing Library

## E2E Testing with Playwright

Use the **e2e-runner** agent for generating and maintaining E2E tests.

```typescript
import { test, expect } from '@playwright/test';

test('user can log in', async ({ page }) => {
  await page.goto('/login');
  await page.fill('[data-testid="email"]', 'user@example.com');
  await page.fill('[data-testid="password"]', 'password');
  await page.click('[data-testid="submit"]');
  await expect(page.locator('[data-testid="dashboard"]')).toBeVisible();
});
```

## Test Organization

```
src/
  components/
    Button.svelte
    Button.test.ts      # Co-located unit tests
  lib/
    utils.ts
    utils.test.ts
tests/
  e2e/                  # E2E tests separate
    login.spec.ts
```

## Coverage

Run with: `vitest --coverage`
Target: 80%+ on all metrics (statements, branches, functions, lines).
