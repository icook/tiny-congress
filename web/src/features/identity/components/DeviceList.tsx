/**
 * DeviceList - Shows devices for the authenticated user with management actions
 */

import { useState } from 'react';
import { IconCheck, IconEdit, IconTrash, IconX } from '@tabler/icons-react';
import { ActionIcon, Badge, Code, Group, Table, Text, TextInput, Tooltip } from '@mantine/core';
import type { DeviceInfo } from '../api';

export interface DeviceListProps {
  devices: DeviceInfo[];
  currentDeviceKid: string | null;
  onRevoke: (kid: string) => void;
  onRename: (kid: string, name: string) => void;
  isRevoking: boolean;
  isRenaming: boolean;
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) {
    return '—';
  }
  return new Date(dateStr).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function DeviceList({
  devices,
  currentDeviceKid,
  onRevoke,
  onRename,
  isRevoking,
  isRenaming,
}: DeviceListProps) {
  const [editingKid, setEditingKid] = useState<string | null>(null);
  const [editName, setEditName] = useState('');

  const startEditing = (kid: string, currentName: string) => {
    setEditingKid(kid);
    setEditName(currentName);
  };

  const cancelEditing = () => {
    setEditingKid(null);
    setEditName('');
  };

  const submitRename = (kid: string) => {
    if (editName.trim()) {
      onRename(kid, editName.trim());
      setEditingKid(null);
    }
  };

  return (
    <Table striped highlightOnHover>
      <Table.Thead>
        <Table.Tr>
          <Table.Th>Name</Table.Th>
          <Table.Th>KID</Table.Th>
          <Table.Th>Created</Table.Th>
          <Table.Th>Last Used</Table.Th>
          <Table.Th>Status</Table.Th>
          <Table.Th>Actions</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {devices.map((device) => {
          const isCurrent = device.device_kid === currentDeviceKid;
          const isRevoked = device.revoked_at !== null;
          const isEditing = editingKid === device.device_kid;

          return (
            <Table.Tr key={device.device_kid}>
              <Table.Td>
                {isEditing ? (
                  <Group gap="xs">
                    <TextInput
                      size="xs"
                      value={editName}
                      onChange={(e) => {
                        setEditName(e.currentTarget.value);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          submitRename(device.device_kid);
                        }
                        if (e.key === 'Escape') {
                          cancelEditing();
                        }
                      }}
                      style={{ width: 150 }}
                    />
                    <ActionIcon
                      size="sm"
                      color="green"
                      variant="subtle"
                      onClick={() => {
                        submitRename(device.device_kid);
                      }}
                      loading={isRenaming}
                    >
                      <IconCheck size={14} />
                    </ActionIcon>
                    <ActionIcon size="sm" color="gray" variant="subtle" onClick={cancelEditing}>
                      <IconX size={14} />
                    </ActionIcon>
                  </Group>
                ) : (
                  <Group gap="xs">
                    <Text size="sm">{device.device_name}</Text>
                    {isCurrent ? (
                      <Badge size="xs" color="blue">
                        Current
                      </Badge>
                    ) : null}
                  </Group>
                )}
              </Table.Td>
              <Table.Td>
                <Code>{device.device_kid.slice(0, 8)}...</Code>
              </Table.Td>
              <Table.Td>
                <Text size="sm">{formatDate(device.created_at)}</Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm">{formatDate(device.last_used_at)}</Text>
              </Table.Td>
              <Table.Td>
                {isRevoked ? (
                  <Badge color="red" size="sm">
                    Revoked
                  </Badge>
                ) : (
                  <Badge color="green" size="sm">
                    Active
                  </Badge>
                )}
              </Table.Td>
              <Table.Td>
                {!isRevoked && !isCurrent ? (
                  <Group gap="xs">
                    <Tooltip label="Rename">
                      <ActionIcon
                        size="sm"
                        variant="subtle"
                        aria-label="Rename"
                        onClick={() => {
                          startEditing(device.device_kid, device.device_name);
                        }}
                      >
                        <IconEdit size={14} />
                      </ActionIcon>
                    </Tooltip>
                    <Tooltip label="Revoke">
                      <ActionIcon
                        size="sm"
                        color="red"
                        variant="subtle"
                        aria-label="Revoke"
                        loading={isRevoking}
                        onClick={() => {
                          onRevoke(device.device_kid);
                        }}
                      >
                        <IconTrash size={14} />
                      </ActionIcon>
                    </Tooltip>
                  </Group>
                ) : null}
              </Table.Td>
            </Table.Tr>
          );
        })}
      </Table.Tbody>
    </Table>
  );
}
