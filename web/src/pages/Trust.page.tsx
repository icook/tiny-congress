/**
 * Trust & Identity dashboard page
 */

import { IconAlertTriangle, IconCheck, IconClock, IconX } from '@tabler/icons-react';
import { Alert, Badge, Card, Loader, Stack, Table, Text, Title } from '@mantine/core';
import { TrustScoreCard, useMyInvites, type Invite } from '@/features/trust';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

function InviteStatusBadge({ invite }: { invite: Invite }) {
  if (invite.accepted_at) {
    return (
      <Badge color="green" leftSection={<IconCheck size={12} />} variant="light">
        Accepted
      </Badge>
    );
  }
  if (new Date(invite.expires_at) < new Date()) {
    return (
      <Badge color="gray" leftSection={<IconX size={12} />} variant="light">
        Expired
      </Badge>
    );
  }
  return (
    <Badge color="blue" leftSection={<IconClock size={12} />} variant="light">
      Pending
    </Badge>
  );
}

export function TrustPage() {
  const { deviceKid, privateKey } = useDevice();
  const { crypto } = useCrypto();

  const invitesQuery = useMyInvites(deviceKid, privateKey, crypto);

  // Defensive safety net — route guard guarantees deviceKid is set
  if (!deviceKid) {
    return null;
  }

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl" px="md">
      <div>
        <Title order={2}>Trust &amp; Identity</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Your trust network and invite history
        </Text>
      </div>

      <TrustScoreCard deviceKid={deviceKid} privateKey={privateKey} wasmCrypto={crypto} />

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Title order={4}>My Invites</Title>

          {invitesQuery.isLoading ? <Loader size="sm" /> : null}

          {invitesQuery.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to load invites: {invitesQuery.error.message}
            </Alert>
          ) : null}

          {invitesQuery.data?.length === 0 ? (
            <Text size="sm" c="dimmed">
              You haven&apos;t sent any invites yet.
            </Text>
          ) : null}

          {(invitesQuery.data?.length ?? 0) > 0 ? (
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Method</Table.Th>
                  <Table.Th>Status</Table.Th>
                  <Table.Th>Expires</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {invitesQuery.data?.map((invite) => (
                  <Table.Tr key={invite.id}>
                    <Table.Td>
                      <Text size="sm">{invite.delivery_method}</Text>
                    </Table.Td>
                    <Table.Td>
                      <InviteStatusBadge invite={invite} />
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm" c="dimmed">
                        {new Date(invite.expires_at).toLocaleDateString()}
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          ) : null}
        </Stack>
      </Card>
    </Stack>
  );
}
