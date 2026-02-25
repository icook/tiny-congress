import { render, screen } from '@test-utils';
import { beforeEach, expect, test, vi } from 'vitest';
import { fetchBuildInfo } from '../api/buildInfo';
import { AboutPage } from './About.page';

vi.mock('../api/buildInfo');

const mockFetchBuildInfo = vi.mocked(fetchBuildInfo);

beforeEach(() => {
  mockFetchBuildInfo.mockReset();
});

test('shows a loading indicator while fetching build info', () => {
  mockFetchBuildInfo.mockReturnValue(new Promise(() => {}));

  render(<AboutPage />);

  expect(screen.getByTestId('build-info-loading')).toBeInTheDocument();
});

test('renders build metadata once loaded', async () => {
  mockFetchBuildInfo.mockResolvedValue({
    version: '1.2.3',
    gitSha: 'abc123',
    buildTime: '2024-01-02T03:04:05Z',
    message: 'deployed from main',
  });

  render(<AboutPage />);

  const version = await screen.findByTestId('api-version');
  const gitSha = await screen.findByTestId('api-git-sha');
  const buildTime = await screen.findByTestId('api-build-time');
  const message = await screen.findByTestId('api-build-message');

  expect(version).toHaveTextContent('1.2.3');
  expect(gitSha).toHaveTextContent('abc123');
  expect(buildTime).toHaveTextContent('2024-01-02T03:04:05Z');
  expect(message).toHaveTextContent('deployed from main');
});

test('renders UI build metadata from compile-time constants', () => {
  mockFetchBuildInfo.mockReturnValue(new Promise(() => {}));

  render(<AboutPage />);

  // Values come from Vite define (GIT_SHA / BUILD_TIME env vars, defaulting to "unknown")
  // so we only assert the elements render — the exact text depends on the environment.
  expect(screen.getByTestId('ui-git-sha')).toBeInTheDocument();
  expect(screen.getByTestId('ui-build-time')).toBeInTheDocument();
});

test('shows an error state when the query fails', async () => {
  mockFetchBuildInfo.mockRejectedValue(new Error('boom'));

  render(<AboutPage />);

  const error = await screen.findByTestId('build-info-error');
  expect(error).toHaveTextContent('boom');
});
