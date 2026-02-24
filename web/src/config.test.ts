import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

describe('getApiBaseUrl', () => {
  const ORIGINAL_TC_ENV = window.__TC_ENV__;

  beforeEach(() => {
    vi.resetModules();
    delete window.__TC_ENV__;
  });

  afterEach(() => {
    window.__TC_ENV__ = ORIGINAL_TC_ENV;
    vi.unstubAllEnvs();
  });

  test('returns runtime config value when window.__TC_ENV__ is set', async () => {
    window.__TC_ENV__ = { VITE_API_URL: 'https://api.prod.example.com' };
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://api.prod.example.com');
  });

  test('falls back to import.meta.env.VITE_API_URL when no runtime config', async () => {
    vi.stubEnv('VITE_API_URL', 'https://api.staging.example.com');
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://api.staging.example.com');
  });

  test('falls back to localhost when neither runtime config nor env var is set', async () => {
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('http://localhost:8080');
  });

  test('runtime config takes precedence over import.meta.env', async () => {
    window.__TC_ENV__ = { VITE_API_URL: 'https://runtime.example.com' };
    vi.stubEnv('VITE_API_URL', 'https://buildtime.example.com');
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://runtime.example.com');
  });

  test('skips empty string in runtime config', async () => {
    window.__TC_ENV__ = { VITE_API_URL: '' };
    vi.stubEnv('VITE_API_URL', 'https://buildtime.example.com');
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://buildtime.example.com');
  });
});
