import { IconHandGrab, IconQrcode } from '@tabler/icons-react';
import { useSearch } from '@tanstack/react-router';
import { Alert, Card, Loader, Stack, Tabs, Title } from '@mantine/core';
import {
  useMyEndorsementsList,
  useRevokeEndorsement,
  useTrustBudget,
} from '@/features/endorsements';
import { AcceptTab } from '@/features/endorsements/components/AcceptTab';
import { EndorsementList } from '@/features/endorsements/components/EndorsementList';
import { GiveTab } from '@/features/endorsements/components/GiveTab';
import { SlotCounter } from '@/features/endorsements/components/SlotCounter';
import { useCryptoRequired } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

export function EndorsePage() {
  const { deviceKid, privateKey } = useDevice();
  const crypto = useCryptoRequired();
  const search = useSearch({ strict: false });

  const budgetQuery = useTrustBudget(deviceKid, privateKey, crypto);
  const endorsementsQuery = useMyEndorsementsList(deviceKid, privateKey, crypto);
  // Safe to pass empty string — mutation won't fire until user clicks revoke,
  // and this page is behind authRequiredLayout so deviceKid is always set.
  const revokeMutation = useRevokeEndorsement(
    deviceKid ?? '',
    privateKey ?? ({} as CryptoKey),
    crypto
  );

  const defaultTab = search.invite ? 'accept' : 'give';

  if (!deviceKid || !privateKey) {
    return <Alert color="red">Not authenticated</Alert>;
  }

  return (
    <Stack gap="md" maw={500} mx="auto" py="md" px="md">
      <Title order={2}>Endorse</Title>

      {budgetQuery.isLoading ? (
        <Loader size="sm" />
      ) : budgetQuery.data ? (
        <SlotCounter
          used={budgetQuery.data.slots_used}
          total={budgetQuery.data.slots_total}
          outOfSlot={budgetQuery.data.out_of_slot_count}
        />
      ) : null}

      <Card withBorder padding="md">
        <Tabs defaultValue={defaultTab}>
          <Tabs.List grow>
            <Tabs.Tab value="give" leftSection={<IconQrcode size={16} />}>
              Give Endorsement
            </Tabs.Tab>
            <Tabs.Tab value="accept" leftSection={<IconHandGrab size={16} />}>
              Accept Endorsement
            </Tabs.Tab>
          </Tabs.List>

          <Tabs.Panel value="give" pt="md">
            <GiveTab
              deviceKid={deviceKid}
              privateKey={privateKey}
              crypto={crypto}
              slotsAvailable={budgetQuery.data?.slots_available ?? 0}
            />
          </Tabs.Panel>

          <Tabs.Panel value="accept" pt="md">
            <AcceptTab
              deviceKid={deviceKid}
              privateKey={privateKey}
              crypto={crypto}
              prefillInviteId={search.invite}
            />
          </Tabs.Panel>
        </Tabs>
      </Card>

      <Card withBorder padding="md">
        <Title order={4} mb="sm">
          Active Endorsements
        </Title>
        {endorsementsQuery.isLoading ? (
          <Loader size="sm" />
        ) : endorsementsQuery.data ? (
          <EndorsementList
            endorsements={endorsementsQuery.data.endorsements}
            onRevoke={(subjectId) => {
              revokeMutation.mutate(subjectId);
            }}
            isRevoking={revokeMutation.isPending}
          />
        ) : (
          <Alert color="red">Failed to load endorsements</Alert>
        )}
      </Card>
    </Stack>
  );
}
