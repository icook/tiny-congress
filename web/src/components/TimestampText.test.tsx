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
  expect(el).toHaveTextContent('2024');

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

test('does not cycle when value is invalid', async () => {
  const user = userEvent.setup();
  render(<TimestampText value="not-a-date" data-testid="ts" />);
  const el = screen.getByTestId('ts');
  await user.click(el);
  expect(el).toHaveTextContent('not-a-date');
});
