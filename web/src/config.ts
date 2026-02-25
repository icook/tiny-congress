/**
 * Runtime environment configuration.
 *
 * In production, docker-entrypoint.sh generates /config.js which sets
 * window.__TC_ENV__ from container environment variables.
 * In local dev, Vite's import.meta.env is used as a fallback.
 */

interface RuntimeConfig {
  VITE_API_URL?: string;
  TC_ENVIRONMENT?: string;
}

declare global {
  interface Window {
    __TC_ENV__?: RuntimeConfig;
  }
}

export function getEnvironment(): string {
  const env = window.__TC_ENV__?.TC_ENVIRONMENT;
  if (!env) {
    return 'production';
  }
  return env;
}

export function getApiBaseUrl(): string {
  const runtime = window.__TC_ENV__?.VITE_API_URL;
  if (runtime) {
    return runtime;
  }

  const buildTime = import.meta.env.VITE_API_URL as string | undefined;
  if (buildTime) {
    return buildTime;
  }

  return 'http://localhost:8080';
}
