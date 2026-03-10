import Markdown from 'react-markdown';
import { Typography } from '@mantine/core';

interface MarkdownContentProps {
  /** Raw markdown string (use Vite `?raw` import) */
  children: string;
}

/** Renders a markdown string with Mantine typography styles. */
export function MarkdownContent({ children }: MarkdownContentProps) {
  return (
    <Typography>
      <Markdown>{children}</Markdown>
    </Typography>
  );
}
