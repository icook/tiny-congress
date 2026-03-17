import {
  IconHeartHandshake,
  IconLogout,
  IconSettings,
  IconShieldHalfFilled,
  IconUser,
} from '@tabler/icons-react';
import { Link, useNavigate } from '@tanstack/react-router';
import { Accordion, Badge, Group, NavLink, Stack, Text, ThemeIcon } from '@mantine/core';
import { useTrustScores } from '@/features/trust';
import { buildVerifierUrl, useVerificationStatus } from '@/features/verification';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

function TrustDot({
  isVerified,
  trustScore,
  username,
}: {
  isVerified: boolean;
  trustScore: { distance: number; path_diversity: number } | null;
  username: string | null;
}) {
  if (isVerified && trustScore) {
    if (trustScore.distance <= 3.0 && trustScore.path_diversity >= 2) {
      return <Badge size="xs" color="violet" circle />;
    }
    if (trustScore.distance <= 6.0 && trustScore.path_diversity >= 1) {
      return <Badge size="xs" color="blue" circle />;
    }
    return <Badge size="xs" color="green" circle />;
  }
  if (isVerified) {
    return <Badge size="xs" color="green" circle />;
  }
  const url = buildVerifierUrl(username ?? '');
  if (url) {
    return <Badge size="xs" color="yellow" circle />;
  }
  return null;
}

interface UserAccordionProps {
  onNavigate?: () => void;
}

export function UserAccordion({ onNavigate }: UserAccordionProps) {
  const { deviceKid, privateKey, username, clearDevice } = useDevice();
  const navigate = useNavigate();
  const { crypto } = useCrypto();
  const verificationQuery = useVerificationStatus(deviceKid, privateKey, crypto);
  const trustScoresQuery = useTrustScores(deviceKid, privateKey, crypto);
  const isVerified = verificationQuery.data?.isVerified ?? false;
  const trustScore = trustScoresQuery.data?.[0] ?? null;

  const handleLogout = () => {
    clearDevice();
    void navigate({ to: '/' });
    onNavigate?.();
  };

  return (
    <Accordion variant="default" chevronPosition="right">
      <Accordion.Item value="user">
        <Accordion.Control>
          <Group gap="xs" wrap="nowrap">
            <ThemeIcon variant="subtle" size="sm">
              <IconUser size={16} />
            </ThemeIcon>
            <Text size="sm" fw={500} truncate>
              {username}
            </Text>
            <TrustDot isVerified={isVerified} trustScore={trustScore} username={username} />
          </Group>
        </Accordion.Control>
        <Accordion.Panel>
          <Stack gap={4}>
            <NavLink
              component={Link}
              to="/trust"
              label="Trust"
              leftSection={<IconShieldHalfFilled size={16} stroke={1.5} />}
              onClick={onNavigate}
            />
            <NavLink
              component={Link}
              to="/endorse"
              label="Endorse"
              leftSection={<IconHeartHandshake size={16} stroke={1.5} />}
              onClick={onNavigate}
            />
            <NavLink
              component={Link}
              to="/settings"
              label="Settings"
              leftSection={<IconSettings size={16} stroke={1.5} />}
              onClick={onNavigate}
            />
            <NavLink
              label="Logout"
              leftSection={<IconLogout size={16} stroke={1.5} />}
              color="red"
              onClick={handleLogout}
            />
          </Stack>
        </Accordion.Panel>
      </Accordion.Item>
    </Accordion>
  );
}
