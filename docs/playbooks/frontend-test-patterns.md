# Frontend Test Patterns

## When to use
- Writing new component tests
- Adding E2E test coverage
- Debugging test failures

## Test types

| Type | Tool | Location | Run command |
|------|------|----------|-------------|
| Unit/Component | Vitest | `web/src/**/*.test.tsx` | `just test-frontend` |
| E2E | Playwright | `web/tests/` | `just test-frontend-e2e` |

## Vitest patterns
Use `@test-utils` for `render`, `screen`, and `userEvent` so Mantine and Query providers are included.

### Basic component test
```typescript
import { render, screen } from '@test-utils';
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
import { render, screen, userEvent } from '@test-utils';

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

## Playwright patterns
Use `./fixtures` so coverage collection and shared helpers stay active (lint enforced).

### Basic E2E test
```typescript
import { test, expect } from './fixtures';

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

### Testing time-dependent code

Use fake timers for deterministic tests involving `setTimeout`, `setInterval`, or time-based UI.

**Vitest:**
```typescript
import { vi } from 'vitest';

describe('TimerComponent', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('updates after delay', () => {
    render(<TimerComponent />);

    vi.advanceTimersByTime(5000); // instant "5 seconds later"

    expect(screen.getByText('Done')).toBeInTheDocument();
  });
});
```

**Playwright:**
```typescript
test('countdown completes', async ({ page }) => {
  await page.clock.install({ time: new Date('2024-01-01T10:00:00') });
  await page.goto('/countdown');

  await page.clock.runFor(5000); // advance 5 seconds

  await expect(page.getByText('Done')).toBeVisible();
});
```

**When to use:** Any test with timers, intervals, countdowns, or animations.
**When NOT to use:** Tests that just wait for async data - use `waitFor` instead.

## Coverage requirements

CI collects coverage for both test types:
- Vitest: V8 coverage via Vitest provider
- Playwright: V8 coverage via instrumentation

Coverage reports uploaded as artifacts.

## Verification
- [ ] Tests pass locally: `just test-frontend-full`
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
- [Test Writing Skill](../skills/test-writing.md) - LLM decision tree for test placement
- [Backend Test Patterns](./backend-test-patterns.md) - Rust/database testing guide
- `web/test-utils/` - Shared test utilities
- `web/vitest.setup.mjs` - Test configuration
- `web/playwright.config.ts` - E2E configuration
