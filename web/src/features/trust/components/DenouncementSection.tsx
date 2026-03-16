import { useState } from 'react';
import { IconAlertTriangle } from '@tabler/icons-react';
import {
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Modal,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { useDenounce, useLookupAccount, useMyDenouncements, type TrustBudget } from '../api';

interface DenouncementSectionProps {
  deviceKid: string | null;
  privateKey: CryptoKey | null;
  wasmCrypto: CryptoModule | null;
  budget: TrustBudget | null | undefined;
}

export function DenouncementSection({
  deviceKid,
  privateKey,
  wasmCrypto,
  budget,
}: DenouncementSectionProps) {
  const [targetUsername, setTargetUsername] = useState('');
  const [reason, setReason] = useState('');
  const [confirmUsername, setConfirmUsername] = useState('');
  const [confirmOpened, { open: openConfirm, close: closeConfirm }] = useDisclosure(false);
  const [lookupError, setLookupError] = useState<string | null>(null);

  const { data: denouncements, isLoading } = useMyDenouncements(deviceKid, privateKey, wasmCrypto);

  const lookupQuery = useLookupAccount(
    deviceKid,
    privateKey,
    wasmCrypto,
    confirmOpened ? targetUsername : ''
  );

  const denounceMutation = useDenounce(
    deviceKid ?? '',
    privateKey ?? (null as unknown as CryptoKey),
    wasmCrypto ?? (null as unknown as CryptoModule)
  );

  const canSubmit =
    targetUsername.trim().length > 0 &&
    reason.trim().length > 0 &&
    budget != null &&
    budget.denouncements_available > 0;

  const handleOpenConfirm = () => {
    setConfirmUsername('');
    setLookupError(null);
    openConfirm();
  };

  const handleDenounce = async () => {
    if (!lookupQuery.data) {
      setLookupError('User not found. Check the username and try again.');
      return;
    }

    try {
      await denounceMutation.mutateAsync({
        target_id: lookupQuery.data.id,
        reason: reason.trim(),
      });
      closeConfirm();
      setTargetUsername('');
      setReason('');
      setConfirmUsername('');
    } catch {
      // error surfaced via denounceMutation.error
    }
  };

  const confirmMatches = confirmUsername === targetUsername;
  const budgetExhausted = budget?.denouncements_available === 0;

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Stack gap="md">
        <Group justify="space-between">
          <Title order={4}>Denouncements</Title>
          {budget != null && (
            <Badge color={budgetExhausted ? 'red' : 'gray'} variant="light">
              {budget.denouncements_used}/{budget.denouncements_total} used
            </Badge>
          )}
        </Group>

        {isLoading ? <Loader size="sm" /> : null}

        {!isLoading && denouncements?.length === 0 ? (
          <Text size="sm" c="dimmed">
            No active denouncements.
          </Text>
        ) : null}

        {(denouncements?.length ?? 0) > 0 ? (
          <Stack gap="xs">
            {denouncements?.map((d) => (
              <Group key={d.id} justify="space-between">
                <Text size="sm" fw={500}>
                  {d.target_username}
                </Text>
                <Text size="xs" c="dimmed">
                  {new Date(d.created_at).toLocaleDateString()}
                </Text>
              </Group>
            ))}
          </Stack>
        ) : null}

        {!budgetExhausted ? (
          <Stack gap="xs">
            <TextInput
              label="Username to denounce"
              placeholder="Enter username"
              value={targetUsername}
              onChange={(e) => {
                setTargetUsername(e.currentTarget.value);
              }}
            />
            <TextInput
              label="Reason"
              placeholder="Why are you withdrawing trust?"
              value={reason}
              onChange={(e) => {
                setReason(e.currentTarget.value);
              }}
            />
            <Button color="red" variant="outline" onClick={handleOpenConfirm} disabled={!canSubmit}>
              File Denouncement
            </Button>
          </Stack>
        ) : null}

        {budgetExhausted ? (
          <Alert color="yellow" icon={<IconAlertTriangle size={16} />}>
            You have used all {budget.denouncements_total} denouncement slots. This action is
            permanent and cannot be undone.
          </Alert>
        ) : null}

        <Modal opened={confirmOpened} onClose={closeConfirm} title="Confirm Denouncement">
          <Stack>
            <Alert color="red" icon={<IconAlertTriangle size={16} />}>
              <Text size="sm" fw={700}>
                This action is irreversible.
              </Text>
              <Text size="sm" mt={4}>
                Denouncing <strong>{targetUsername}</strong> will permanently use 1 of your{' '}
                {budget?.denouncements_total} denouncement slots. Any existing endorsement of this
                user will be revoked.
              </Text>
            </Alert>

            {lookupQuery.isLoading ? <Loader size="sm" /> : null}

            {lookupQuery.isError || lookupError ? (
              <Alert color="red" icon={<IconAlertTriangle size={16} />}>
                {lookupError ?? 'User not found. Check the username and try again.'}
              </Alert>
            ) : null}

            {denounceMutation.isError ? (
              <Alert color="red" icon={<IconAlertTriangle size={16} />}>
                {denounceMutation.error instanceof Error
                  ? denounceMutation.error.message
                  : 'Denouncement failed. Please try again.'}
              </Alert>
            ) : null}

            <TextInput
              label={`Type "${targetUsername}" to confirm`}
              placeholder={targetUsername}
              value={confirmUsername}
              onChange={(e) => {
                setConfirmUsername(e.currentTarget.value);
              }}
            />

            <Group justify="flex-end">
              <Button variant="default" onClick={closeConfirm}>
                Cancel
              </Button>
              <Button
                color="red"
                onClick={() => {
                  void handleDenounce();
                }}
                loading={denounceMutation.isPending}
                disabled={
                  !confirmMatches ||
                  lookupQuery.isLoading ||
                  lookupQuery.isError ||
                  !lookupQuery.data
                }
              >
                Denounce
              </Button>
            </Group>
          </Stack>
        </Modal>
      </Stack>
    </Card>
  );
}
