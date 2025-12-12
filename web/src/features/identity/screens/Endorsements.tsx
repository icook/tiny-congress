/**
 * Endorsements page - view my endorsements and create new ones
 */

import { useCallback, useEffect, useState } from 'react';
import { IconAlertCircle, IconPlus, IconStar } from '@tabler/icons-react';
import {
  Alert,
  Button,
  Container,
  Group,
  Modal,
  Paper,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { ApiError, getEndorsements, revokeEndorsement, type Endorsement } from '../api/client';
import { EndorsementEditor, EndorsementItem } from '../components/EndorsementEditor';
import { deriveKid, getDevicePrivateKey, getDevicePublicKey, signEnvelope } from '../keys';
import { getSession } from '../state/session';

export function Endorsements() {
  const [endorsements, setEndorsements] = useState<Endorsement[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [createModalOpened, { open: openCreateModal, close: closeCreateModal }] =
    useDisclosure(false);
  const [subjectAccountId, setSubjectAccountId] = useState('');

  const session = getSession();

  const fetchEndorsements = useCallback(async () => {
    if (!session?.accountId) {
      setError('Please login to view endorsements');
      setLoading(false);
      return;
    }

    try {
      // For now, fetch endorsements where user is author
      // In a real app, you might have a separate endpoint for "my endorsements"
      const [myEndorsements] = await getEndorsements(session.accountId);
      setEndorsements(myEndorsements);
      setError(null);
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to load endorsements');
      }
    } finally {
      setLoading(false);
    }
  }, [session?.accountId]);

  useEffect(() => {
    fetchEndorsements();
  }, [fetchEndorsements]);

  const handleRevoke = async (endorsementId: string) => {
    if (!session?.sessionToken || !session?.accountId || !session?.deviceId) {
      setError('Please login to revoke endorsements');
      return;
    }

    try {
      // Get device key for signing
      const privateKey = await getDevicePrivateKey(session.accountId);
      const publicKey = await getDevicePublicKey(session.accountId);
      const deviceKid = deriveKid(publicKey);

      // Create revocation envelope
      const envelope = signEnvelope(
        'EndorsementRevocation',
        { endorsement_id: endorsementId },
        {
          account_id: session.accountId,
          device_id: session.deviceId,
          kid: deviceKid,
        },
        privateKey
      );

      await revokeEndorsement(session.sessionToken, endorsementId, {
        account_id: session.accountId,
        device_id: session.deviceId,
        envelope,
      });

      // Remove from local state
      setEndorsements((prev) => prev.filter((e) => e.id !== endorsementId));
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.message);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Failed to revoke endorsement');
      }
    }
  };

  const handleCreateSuccess = () => {
    closeCreateModal();
    setSubjectAccountId('');
    fetchEndorsements();
  };

  return (
    <Container size="md" mt="xl">
      <Paper withBorder shadow="md" p="xl" radius="md">
        <Group justify="space-between" mb="lg">
          <Group>
            <IconStar size={28} />
            <Title order={2}>My Endorsements</Title>
          </Group>
          <Button leftSection={<IconPlus size={16} />} onClick={openCreateModal}>
            New Endorsement
          </Button>
        </Group>

        {error && (
          <Alert icon={<IconAlertCircle size={16} />} color="red" mb="md">
            {error}
          </Alert>
        )}

        {loading ? (
          <Text>Loading endorsements...</Text>
        ) : (
          <Stack gap="md">
            {endorsements.length === 0 ? (
              <Text c="dimmed">No endorsements yet. Create one to get started.</Text>
            ) : (
              endorsements.map((endorsement) => (
                <EndorsementItem
                  key={endorsement.id}
                  endorsement={endorsement}
                  canRevoke={endorsement.author_account_id === session?.accountId}
                  onRevoke={handleRevoke}
                />
              ))
            )}
          </Stack>
        )}
      </Paper>

      <Modal
        opened={createModalOpened}
        onClose={closeCreateModal}
        title="Create New Endorsement"
        size="lg"
      >
        <Stack gap="md">
          <TextInput
            label="Subject Account ID"
            placeholder="Enter the account ID to endorse"
            value={subjectAccountId}
            onChange={(e) => setSubjectAccountId(e.target.value)}
            required
          />

          {subjectAccountId && (
            <EndorsementEditor
              subjectAccountId={subjectAccountId}
              onSuccess={handleCreateSuccess}
              onCancel={closeCreateModal}
            />
          )}
        </Stack>
      </Modal>
    </Container>
  );
}
