import { useCallback, useEffect, useRef, useState } from 'react';
import { IconCamera, IconCameraOff, IconCheck } from '@tabler/icons-react';
import QrScanner from 'qr-scanner';
import { Alert, Button, Divider, Group, Stack, Text, TextInput } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { useAcceptInvite } from '../api';

interface AcceptTabProps {
  deviceKid: string;
  privateKey: CryptoKey;
  crypto: CryptoModule;
  prefillInviteId?: string;
}

function extractInviteId(input: string): string | null {
  try {
    const url = new URL(input, window.location.origin);
    const invite = url.searchParams.get('invite');
    if (invite) {
      return invite;
    }
  } catch {
    // Not a URL
  }
  const uuidMatch = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i.exec(input);
  return uuidMatch ? uuidMatch[0] : null;
}

export function AcceptTab({ deviceKid, privateKey, crypto, prefillInviteId }: AcceptTabProps) {
  const acceptMutation = useAcceptInvite(deviceKid, privateKey, crypto);
  const [pasteValue, setPasteValue] = useState('');
  const [scanning, setScanning] = useState(false);
  const [accepted, setAccepted] = useState(false);
  const [acceptedEndorser, setAcceptedEndorser] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const scannerRef = useRef<QrScanner | null>(null);

  const handleAccept = useCallback(
    async (inviteId: string) => {
      try {
        const result = await acceptMutation.mutateAsync(inviteId);
        setAccepted(true);
        setAcceptedEndorser(result.endorser_id);
        notifications.show({
          title: 'Endorsement received!',
          message: `Endorsed by ${result.endorser_id}`,
          color: 'green',
        });
      } catch (e) {
        const msg = e instanceof Error ? e.message : 'Unknown error';
        const display = msg.includes('not found')
          ? 'This invite has expired or was already used.'
          : msg;
        notifications.show({
          title: 'Failed to accept',
          message: display,
          color: 'red',
        });
      }
    },
    // acceptMutation is stable (same deviceKid/privateKey/crypto for lifetime of component)
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [deviceKid]
  );

  useEffect(() => {
    if (prefillInviteId && !accepted) {
      void handleAccept(prefillInviteId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefillInviteId]);

  const startScanner = useCallback(async () => {
    if (!videoRef.current) {
      return;
    }
    setScanning(true);

    const scanner = new QrScanner(
      videoRef.current,
      (result) => {
        const inviteId = extractInviteId(result.data);
        if (inviteId) {
          scanner.stop();
          scanner.destroy();
          scannerRef.current = null;
          setScanning(false);
          void handleAccept(inviteId);
        }
      },
      {
        preferredCamera: 'environment',
        highlightScanRegion: true,
        highlightCodeOutline: true,
      }
    );

    scannerRef.current = scanner;
    try {
      await scanner.start();
    } catch {
      setScanning(false);
      notifications.show({
        title: 'Camera error',
        message: 'Could not access camera. Use the paste option below.',
        color: 'yellow',
      });
    }
  }, [handleAccept]);

  const stopScanner = useCallback(() => {
    if (scannerRef.current) {
      scannerRef.current.stop();
      scannerRef.current.destroy();
      scannerRef.current = null;
    }
    setScanning(false);
  }, []);

  useEffect(() => {
    return () => {
      if (scannerRef.current) {
        scannerRef.current.stop();
        scannerRef.current.destroy();
      }
    };
  }, []);

  const handlePaste = () => {
    const inviteId = extractInviteId(pasteValue.trim());
    if (!inviteId) {
      notifications.show({
        title: 'Invalid link',
        message: 'Could not find an invite ID in the pasted text.',
        color: 'red',
      });
      return;
    }
    void handleAccept(inviteId);
  };

  if (accepted) {
    return (
      <Stack align="center" py="lg">
        <IconCheck size={48} color="var(--mantine-color-green-6)" />
        <Text size="lg" fw={600}>
          Endorsement received!
        </Text>
        {acceptedEndorser ? (
          <Text size="sm" c="dimmed">
            From {acceptedEndorser}
          </Text>
        ) : null}
        <Button
          variant="subtle"
          onClick={() => {
            setAccepted(false);
          }}
        >
          Accept Another
        </Button>
      </Stack>
    );
  }

  return (
    <Stack gap="md" py="md">
      <Stack align="center" gap="sm">
        {/* eslint-disable-next-line jsx-a11y/media-has-caption -- QR scanner video feed, no captions applicable */}
        <video
          ref={videoRef}
          style={{
            width: '100%',
            maxWidth: 350,
            borderRadius: 8,
            display: scanning ? 'block' : 'none',
          }}
        />
        {!scanning ? (
          <Button
            leftSection={<IconCamera size={18} />}
            onClick={() => void startScanner()}
            loading={acceptMutation.isPending}
            size="lg"
          >
            Scan QR Code
          </Button>
        ) : (
          <Button
            leftSection={<IconCameraOff size={18} />}
            onClick={stopScanner}
            color="red"
            variant="light"
          >
            Stop Scanner
          </Button>
        )}
      </Stack>

      <Divider label="or paste an invite link" labelPosition="center" />

      <Group gap="xs" align="flex-end">
        <TextInput
          placeholder="Paste invite link or ID"
          value={pasteValue}
          onChange={(e) => {
            setPasteValue(e.currentTarget.value);
          }}
          style={{ flex: 1 }}
        />
        <Button
          onClick={handlePaste}
          loading={acceptMutation.isPending}
          disabled={!pasteValue.trim()}
        >
          Accept
        </Button>
      </Group>

      {acceptMutation.isError ? (
        <Alert color="red" title="Error">
          {acceptMutation.error instanceof Error
            ? acceptMutation.error.message
            : 'Failed to accept invite'}
        </Alert>
      ) : null}
    </Stack>
  );
}
