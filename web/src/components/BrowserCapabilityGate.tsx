/**
 * BrowserCapabilityGate - Probes for Ed25519 Web Crypto support on mount.
 * If the browser does not support Ed25519 key generation, renders a friendly
 * "unsupported browser" message instead of children.
 */

import { useEffect, useState, type ReactNode } from 'react';
import { IconAlertTriangle } from '@tabler/icons-react';
import { Alert, Center, List, Loader, Stack, Text } from '@mantine/core';

interface BrowserCapabilityGateProps {
  children: ReactNode;
}

type CapabilityState = 'checking' | 'supported' | 'unsupported';

const BROWSER_REQUIREMENTS = ['Chrome 113+', 'Firefox 130+', 'Safari 17+', 'Edge 113+'];

export function BrowserCapabilityGate({ children }: BrowserCapabilityGateProps) {
  const [state, setState] = useState<CapabilityState>('checking');

  useEffect(() => {
    async function checkCapabilities() {
      try {
        await crypto.subtle.generateKey('Ed25519', false, ['sign']);
        setState('supported');
      } catch {
        setState('unsupported');
      }
    }

    void checkCapabilities();
  }, []);

  if (state === 'checking') {
    return (
      <Center h="100vh">
        <Loader size="sm" />
      </Center>
    );
  }

  if (state === 'unsupported') {
    return (
      <Center h="100vh" p="xl">
        <Stack maw={500} gap="md">
          <Alert
            icon={<IconAlertTriangle size={20} />}
            title="Browser not supported"
            color="red"
            variant="filled"
          >
            Your browser does not support the cryptographic features TinyCongress requires. Please
            upgrade to a supported browser.
          </Alert>
          <Text size="sm">
            TinyCongress uses Ed25519 public-key cryptography to generate and manage your identity
            keys directly in your browser. This requires a modern browser with Web Crypto API
            support.
          </Text>
          <Text size="sm" fw={600}>
            Minimum supported versions:
          </Text>
          <List size="sm" withPadding>
            {BROWSER_REQUIREMENTS.map((req) => (
              <List.Item key={req}>{req}</List.Item>
            ))}
          </List>
        </Stack>
      </Center>
    );
  }

  return <>{children}</>;
}
