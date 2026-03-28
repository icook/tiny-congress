import { useState } from 'react';
import {
  Badge,
  Button,
  Card,
  FileInput,
  Group,
  Image,
  SegmentedControl,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';
import { useSubmitMeme, type Round, type Submission } from '../api';
import { RoundCountdown } from './RoundCountdown';

interface Props {
  roomId: string;
  round: Round;
}

type ContentMode = 'url' | 'image';

function SubmissionPreview({ submission }: { submission: Submission }) {
  return (
    <Card withBorder padding="sm" radius="sm">
      <Stack gap="xs">
        <Group gap="xs">
          <Badge color="green" variant="filled" size="sm">
            Submitted
          </Badge>
          <Text size="xs" c="dimmed">
            {submission.content_type === 'url' ? 'URL' : 'Image'}
          </Text>
        </Group>
        {submission.content_type === 'url' && submission.url ? (
          <Text size="sm" truncate>
            {submission.url}
          </Text>
        ) : null}
        {submission.content_type === 'image' && submission.image_key ? (
          <Image
            src={submission.image_key}
            alt="Submitted meme"
            maw={300}
            radius="sm"
            fallbackSrc="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
          />
        ) : null}
        {submission.caption ? (
          <Text size="sm" c="dimmed">
            {submission.caption}
          </Text>
        ) : null}
      </Stack>
    </Card>
  );
}

export function SubmitView({ roomId, round }: Props) {
  const { deviceKid, privateKey } = useDevice();
  const { crypto } = useCrypto();
  const submitMutation = useSubmitMeme(roomId);

  const [mode, setMode] = useState<ContentMode>('url');
  const [url, setUrl] = useState('');
  const [imageFile, setImageFile] = useState<File | null>(null);
  const [caption, setCaption] = useState('');
  const [submitted, setSubmitted] = useState<Submission | null>(null);

  const isAuthenticated = Boolean(deviceKid && privateKey && crypto);
  const hasContent = mode === 'url' ? url.trim().length > 0 : imageFile !== null;

  const handleSubmit = () => {
    if (!deviceKid || !privateKey || !crypto) {
      return;
    }
    if (!hasContent) {
      return;
    }

    const body =
      mode === 'url'
        ? {
            content_type: 'url' as const,
            url: url.trim(),
            caption: caption.trim() || undefined,
          }
        : {
            content_type: 'image' as const,
            // image_key would be set after upload; placeholder for now
            image_key: imageFile?.name,
            caption: caption.trim() || undefined,
          };

    submitMutation.mutate(
      { body, deviceKid, privateKey, wasmCrypto: crypto },
      {
        onSuccess: (result) => {
          setSubmitted(result);
        },
      }
    );
  };

  if (submitted) {
    return (
      <Stack gap="md" mt="md">
        <Title order={4}>Your submission</Title>
        <SubmissionPreview submission={submitted} />
        <Text size="sm" c="dimmed">
          Ranking opens when the submission window closes.
        </Text>
        <RoundCountdown deadline={round.rank_opens_at} label="Ranking opens" />
      </Stack>
    );
  }

  return (
    <Stack gap="md" mt="md">
      <Group justify="space-between" wrap="nowrap">
        <Title order={4}>Submit a meme</Title>
        <RoundCountdown deadline={round.rank_opens_at} label="Closes" />
      </Group>

      {!isAuthenticated ? (
        <Text size="sm" c="dimmed">
          Sign in to submit a meme.
        </Text>
      ) : (
        <>
          <SegmentedControl
            value={mode}
            onChange={(v) => {
              setMode(v as ContentMode);
            }}
            data={[
              { label: 'URL', value: 'url' },
              { label: 'Image', value: 'image' },
            ]}
            size="sm"
          />

          {mode === 'url' ? (
            <TextInput
              label="Meme URL"
              placeholder="https://example.com/meme.jpg"
              value={url}
              onChange={(e) => {
                setUrl(e.currentTarget.value);
              }}
            />
          ) : (
            <FileInput
              label="Upload image"
              placeholder="Click to select file"
              accept="image/*"
              value={imageFile}
              onChange={setImageFile}
            />
          )}

          <TextInput
            label="Caption (optional)"
            placeholder="Add a caption..."
            value={caption}
            onChange={(e) => {
              setCaption(e.currentTarget.value);
            }}
            maxLength={280}
            rightSection={
              <Text size="xs" c="dimmed">
                {caption.length}/280
              </Text>
            }
            rightSectionWidth={60}
          />

          {mode === 'url' && url.trim() ? (
            <Card withBorder padding="sm" radius="sm">
              <Stack gap="xs">
                <Text size="xs" c="dimmed" fw={600}>
                  Preview
                </Text>
                <Text size="sm" truncate>
                  {url}
                </Text>
                {caption ? <Text size="sm">{caption}</Text> : null}
              </Stack>
            </Card>
          ) : null}

          {mode === 'image' && imageFile ? (
            <Card withBorder padding="sm" radius="sm">
              <Stack gap="xs">
                <Text size="xs" c="dimmed" fw={600}>
                  Preview
                </Text>
                <Image src={URL.createObjectURL(imageFile)} alt="Preview" maw={300} radius="sm" />
                {caption ? <Text size="sm">{caption}</Text> : null}
              </Stack>
            </Card>
          ) : null}

          {submitMutation.error ? (
            <Text size="sm" c="red">
              {submitMutation.error.message}
            </Text>
          ) : null}

          <Button onClick={handleSubmit} disabled={!hasContent} loading={submitMutation.isPending}>
            Submit
          </Button>
        </>
      )}
    </Stack>
  );
}
