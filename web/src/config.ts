/**
 * Runtime environment configuration.
 *
 * In production, docker-entrypoint.sh generates /config.js which sets
 * window.__TC_ENV__ from container environment variables.
 * In local dev, Vite's import.meta.env is used as a fallback.
 */

interface RuntimeConfig {
  VITE_API_URL?: string;
}

declare global {
  interface Window {
    __TC_ENV__?: RuntimeConfig;
  }
}

export function getApiBaseUrl(): string {
  return (
    window.__TC_ENV__?.VITE_API_URL ||
    (import.meta.env.VITE_API_URL as string | undefined) ||
    'http://localhost:8080'
  );
}
