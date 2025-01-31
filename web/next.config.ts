import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: 'standalone',
  telemetry: {
    telemetryDisabled: true,
  },
};

export default nextConfig;
