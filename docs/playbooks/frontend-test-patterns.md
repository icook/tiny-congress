# Frontend Test Patterns

## When to use
- Writing new component tests
- Adding E2E test coverage
- Debugging test failures

## Test types

| Type | Tool | Location | Run command |
|------|------|----------|-------------|
| Unit/Component | Vitest | `web/src/**/*.test.tsx` | `yarn vitest` |
| E2E | Playwright | `web/tests/` | `yarn playwright:test` |

## Vitest patterns

### Basic component test
```typescript
import { render, screen } from '@testing-library/react';
import { MyComponent } from './MyComponent';

describe('MyComponent', () => {
  it('renders content', () => {
    render(<MyComponent title="Hello" />);
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });
});
```

### With user interaction
```typescript
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

it('handles click', async () => {
  const user = userEvent.setup();
  const onClick = vi.fn();

  render(<Button onClick={onClick}>Click me</Button>);
  await user.click(screen.getByRole('button'));

  expect(onClick).toHaveBeenCalledOnce();
});
```

### Mocking hooks/modules
```typescript
vi.mock('@/hooks/useApi', () => ({
  useApi: () => ({ data: mockData, loading: false }),
}));
```

### Using shared mocks
```typescript
// Import from test-utils
import { mockUser, mockSession } from '@/test-utils/mocks';
```

## Playwright patterns

### Basic E2E test
```typescript
import { test, expect } from '@playwright/test';

test('user can navigate to dashboard', async ({ page }) => {
  await page.goto('/');
  await page.click('text=Dashboard');
  await expect(page).toHaveURL('/dashboard');
});
```

### Selecting elements
```typescript
// Prefer accessible selectors
await page.getByRole('button', { name: 'Submit' }).click();
await page.getByLabel('Email').fill('test@example.com');
await page.getByTestId('user-menu').click();  // Last resort
```

### Waiting for state
```typescript
// Wait for network
await page.waitForResponse(resp => resp.url().includes('/api/users'));

// Wait for element
await expect(page.getByText('Success')).toBeVisible();
```

## Coverage requirements

CI collects coverage for both test types:
- Vitest: Standard Istanbul coverage
- Playwright: V8 coverage via instrumentation

Coverage reports uploaded as artifacts.

## Verification
- [ ] Tests pass locally: `yarn test`
- [ ] No flaky tests (run 3x)
- [ ] Coverage not decreased
- [ ] Tests isolated (no order dependency)

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| "Unable to find element" | Async not awaited | Add `await` or `waitFor` |
| "act() warning" | State update outside act | Wrap in `act()` or use `userEvent` |
| Flaky timeout | Slow CI | Increase timeout or wait for specific state |
| "Target closed" | Page navigated during test | Wait for navigation to complete |

## See also
- `web/test-utils/` - Shared test utilities
- `web/vitest.setup.mjs` - Test configuration
- `web/playwright.config.ts` - E2E configuration
