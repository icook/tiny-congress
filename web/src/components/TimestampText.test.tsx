import { render, screen, userEvent } from '@test-utils';
import { expect, test } from 'vitest';
import { TimestampText } from './TimestampText';

const TIMESTAMP = '2024-06-15T12:30:00Z';

test('renders a formatted local time by default', () => {
  render(<TimestampText value={TIMESTAMP} data-testid="ts" />);
  const el = screen.getByTestId('ts');
  // Should contain "2024" and "Jun" (local formatted) — not the raw ISO string
  expect(el).toHaveTextContent('2024');
  expect(el).toHaveTextContent('Jun');
});

test('cycles to UTC on click', async () => {
  const user = userEvent.setup();
  render(<TimestampText value={TIMESTAMP} data-testid="ts" />);

  const el = screen.getByTestId('ts');
  // Capture local text before cycling
  const snapshot = el.textContent;
  expect(snapshot).toContain('2024');

  // Click → UTC
  await user.click(el);
  expect(el).toHaveTextContent('UTC');

  // Click → relative
  await user.click(el);
  expect(el).toHaveTextContent(/ago|year|month|day|hour|minute|second/i);

  // Click → back to local
  await user.click(el);
  expect(el).toHaveTextContent(snapshot);
});

test('respects defaultMode prop', () => {
  render(<TimestampText value={TIMESTAMP} defaultMode="utc" data-testid="ts" />);
  expect(screen.getByTestId('ts')).toHaveTextContent('UTC');
});

test('falls back to raw string for invalid timestamps', () => {
  render(<TimestampText value="unknown" data-testid="ts" />);
  expect(screen.getByTestId('ts')).toHaveTextContent('unknown');
});

test('cycles via keyboard (Enter and Space)', async () => {
  const user = userEvent.setup();
  render(<TimestampText value={TIMESTAMP} data-testid="ts" />);
  const el = screen.getByTestId('ts');

  // Tab to focus the element
  await user.tab();
  expect(el).toHaveFocus();

  // Enter → UTC
  await user.keyboard('{Enter}');
  expect(el).toHaveTextContent('UTC');

  // Space → relative
  await user.keyboard(' ');
  expect(el).toHaveTextContent(/ago|year|month|day|hour|minute|second/i);

  // Other keys should not cycle
  await user.keyboard('a');
  expect(el).toHaveTextContent(/ago|year|month|day|hour|minute|second/i);
});

test('does not cycle when value is invalid', async () => {
  const user = userEvent.setup();
  render(<TimestampText value="not-a-date" data-testid="ts" />);
  const el = screen.getByTestId('ts');
  await user.click(el);
  expect(el).toHaveTextContent('not-a-date');
});
