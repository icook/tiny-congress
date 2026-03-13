import { useState } from 'react';
import { IconCheck, IconCopy } from '@tabler/icons-react';
import { QRCodeSVG } from 'qrcode.react';
import { Alert, Button, CopyButton, Group, Stack, Text, Tooltip } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { useCreateInvite } from '../api';

interface GiveTabProps {
  deviceKid: string;
  privateKey: CryptoKey;
  crypto: CryptoModule;
  slotsAvailable: number;
}

export function GiveTab({ deviceKid, privateKey, crypto, slotsAvailable }: GiveTabProps) {
  const createInviteMutation = useCreateInvite(deviceKid, privateKey, crypto);
  const [inviteUrl, setInviteUrl] = useState<string | null>(null);
  const [expiresAt, setExpiresAt] = useState<string | null>(null);

  const handleCreate = () => {
    void createInviteMutation
      .mutateAsync({
        envelope: btoa('endorsement-invite'),
        delivery_method: 'qr',
        attestation: { method: 'physical_qr' },
      })
      .then((result) => {
        const url = `${window.location.origin}/endorse?invite=${result.id}`;
        setInviteUrl(url);
        setExpiresAt(result.expires_at);
      })
      .catch((e: unknown) => {
        notifications.show({
          title: 'Failed to create invite',
          message: e instanceof Error ? e.message : 'Unknown error',
          color: 'red',
        });
      });
  };

  if (slotsAvailable <= 0) {
    return (
      <Alert color="yellow" title="No slots available">
        All endorsement slots used. Revoke an existing endorsement to endorse someone new.
      </Alert>
    );
  }

  return (
    <Stack align="center" gap="md" py="md">
      {inviteUrl == null ? (
        <Button onClick={handleCreate} loading={createInviteMutation.isPending} size="lg">
          Create Endorsement Invite
        </Button>
      ) : (
        <>
          <QRCodeSVG value={inviteUrl} size={250} level="M" />
          <Text size="xs" c="dimmed" ta="center" maw={300} style={{ wordBreak: 'break-all' }}>
            {inviteUrl}
          </Text>
          {expiresAt != null && (
            <Text size="xs" c="dimmed">
              Expires {new Date(expiresAt).toLocaleDateString()}
            </Text>
          )}
          <Group>
            <CopyButton value={inviteUrl}>
              {({ copied, copy }) => (
                <Tooltip label={copied ? 'Copied' : 'Copy link'}>
                  <Button
                    variant="light"
                    leftSection={copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
                    onClick={copy}
                    color={copied ? 'teal' : undefined}
                  >
                    {copied ? 'Copied' : 'Copy Link'}
                  </Button>
                </Tooltip>
              )}
            </CopyButton>
          </Group>
          <Button
            variant="subtle"
            onClick={() => {
              setInviteUrl(null);
            }}
          >
            Create Another
          </Button>
        </>
      )}
    </Stack>
  );
}
