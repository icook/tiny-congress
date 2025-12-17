# Fixing Flaky Tests

## When to Use

- CI reports flaky tests (passed on retry)
- Tests fail intermittently in local development
- Test suite reliability needs improvement

## What is a Flaky Test?

A flaky test is one that passes sometimes and fails other times without code changes. In our CI, tests that fail initially but pass on retry are flagged as flaky.

## Common Causes

### 1. Timing Issues

**Symptom:** Test fails waiting for element/state that hasn't rendered yet.

**Fix:** Use Playwright's built-in waiting:
```typescript
// BAD: Fixed timeout
await page.waitForTimeout(1000);
await page.click('#submit');

// GOOD: Wait for element to be actionable
await page.click('#submit'); // Auto-waits

// GOOD: Explicit wait for condition
await expect(page.locator('#result')).toBeVisible();
```

### 2. Race Conditions

**Symptom:** Test depends on order of async operations.

**Fix:** Wait for stable state:
```typescript
// BAD: Assumes data loaded
await page.goto('/dashboard');
await page.click('.item');

// GOOD: Wait for data to load
await page.goto('/dashboard');
await expect(page.locator('.item')).toHaveCount(3);
await page.click('.item');
```

### 3. Shared State

**Symptom:** Test passes alone, fails with other tests.

**Fix:** Isolate test data:
```typescript
// BAD: Uses shared data
test('delete item', async () => {
  await page.click('[data-testid="delete-item-1"]');
});

// GOOD: Create test-specific data
test('delete item', async () => {
  const itemId = await createTestItem();
  await page.click(`[data-testid="delete-item-${itemId}"]`);
});
```

### 4. Network Timing

**Symptom:** Test fails when API responses are slow.

**Fix:** Wait for network idle or specific responses:
```typescript
// Wait for API response
await Promise.all([
  page.waitForResponse('/api/data'),
  page.click('#load-data'),
]);
```

### 5. Animation/Transition Issues

**Symptom:** Click happens during animation.

**Fix:** Wait for animations or disable them:
```typescript
// Option 1: Wait for animation to complete
await page.locator('.modal').waitFor({ state: 'visible' });
await expect(page.locator('.modal')).toHaveCSS('opacity', '1');

// Option 2: Disable animations in test config
// playwright.config.ts
use: {
  // Disable CSS animations
  contextOptions: {
    reducedMotion: 'reduce',
  },
}
```

## Debugging Steps

### 1. Run Test in Headed Mode
```bash
cd web && yarn playwright test --headed tests/e2e/flaky-test.spec.ts
```

### 2. Enable Tracing
```bash
yarn playwright test --trace on tests/e2e/flaky-test.spec.ts
```

### 3. Run Multiple Times
```bash
# Run 10 times to reproduce flakiness
for i in {1..10}; do yarn playwright test tests/e2e/flaky-test.spec.ts || break; done
```

### 4. Check CI Artifacts

Download the `playwright-artifacts` from the failed CI run:
- `playwright-report/` - HTML report with screenshots
- `test-results/` - Traces and videos of failures

## Tagging Known Flaky Tests

If a test is known flaky and a fix isn't immediately available:

```typescript
// Mark as flaky with skip or fixme
test.fixme('known flaky - issue #123', async ({ page }) => {
  // ...
});

// Or add annotation
test('sometimes flaky', async ({ page }) => {
  test.info().annotations.push({ type: 'flaky', description: 'Timing issue - see #123' });
  // ...
});
```

## Prevention

1. **Use Playwright's auto-waiting** - Avoid manual timeouts
2. **Isolate test data** - Each test should create its own data
3. **Wait for specific conditions** - Not arbitrary timeouts
4. **Review CI flakiness reports** - Address flaky tests promptly
5. **Run tests multiple times locally** before merging

## See Also

- [Playwright Best Practices](https://playwright.dev/docs/best-practices)
- `docs/playbooks/frontend-test-patterns.md` - General frontend testing
- `web/playwright.config.ts` - Test configuration
