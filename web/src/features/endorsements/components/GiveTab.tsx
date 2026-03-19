import { useState } from 'react';
import { IconCheck, IconCopy } from '@tabler/icons-react';
import { QRCodeSVG } from 'qrcode.react';
import {
  Alert,
  Badge,
  Button,
  CopyButton,
  Group,
  Select,
  Stack,
  Text,
  Tooltip,
} from '@mantine/core';
import { notifications } from '@mantine/notifications';
import {
  computeWeight,
  DELIVERY_METHODS,
  RELATIONSHIP_DEPTHS,
  weightLabel,
  type DeliveryMethod,
  type RelationshipDepth,
} from '@/api/endorsementWeight';
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
  const [deliveryMethod, setDeliveryMethod] = useState<DeliveryMethod>('qr');
  const [relationshipDepth, setRelationshipDepth] = useState<RelationshipDepth>('years');

  const weight = computeWeight(deliveryMethod, relationshipDepth);
  const label = weightLabel(weight);
  const weightColor = weight >= 0.8 ? 'green' : weight >= 0.4 ? 'yellow' : 'orange';

  const handleCreate = () => {
    void createInviteMutation
      .mutateAsync({
        envelope: btoa('endorsement-invite'),
        delivery_method: deliveryMethod,
        relationship_depth: relationshipDepth,
        attestation: { method: deliveryMethod, relationship_depth: relationshipDepth },
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

  return (
    <Stack gap="md" py="md">
      {inviteUrl == null ? (
        <>
          {slotsAvailable <= 0 && (
            <Alert color="blue" title="Out-of-slot endorsement">
              All trust graph slots are used. This endorsement will be stored but won&apos;t
              contribute to trust scores. It can still grant room access.
            </Alert>
          )}
          <Select
            label="How are you connecting?"
            description="How you're exchanging this invite affects endorsement strength."
            data={DELIVERY_METHODS.map((m) => ({ value: m.value, label: m.label }))}
            value={deliveryMethod}
            onChange={(v) => {
              if (v) {
                setDeliveryMethod(v as DeliveryMethod);
              }
            }}
          />
          <Select
            label="How long have you known this person?"
            data={RELATIONSHIP_DEPTHS.map((d) => ({ value: d.value, label: d.label }))}
            value={relationshipDepth}
            onChange={(v) => {
              if (v) {
                setRelationshipDepth(v as RelationshipDepth);
              }
            }}
          />
          <Group gap="xs" align="center">
            <Text size="sm" c="dimmed">
              Endorsement strength:
            </Text>
            <Badge color={weightColor} variant="light">
              {label} ({(weight * 100).toFixed(0)}%)
            </Badge>
          </Group>
          <Button onClick={handleCreate} loading={createInviteMutation.isPending} size="lg">
            Create Endorsement Invite
          </Button>
        </>
      ) : (
        <Stack align="center" gap="md">
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
        </Stack>
      )}
    </Stack>
  );
}
