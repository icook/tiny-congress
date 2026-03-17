import Markdown, { type Components } from 'react-markdown';
import rehypeRaw from 'rehype-raw';
import rehypeSlug from 'rehype-slug';
import remarkGfm from 'remark-gfm';
import { Anchor, Typography } from '@mantine/core';

interface MarkdownContentProps {
  /** Raw markdown string (use Vite `?raw` import) */
  children: string;
}

const components: Components = {
  a: ({ href, children: linkChildren }) => (
    <Anchor href={href} target={href?.startsWith('http') ? '_blank' : undefined}>
      {linkChildren}
    </Anchor>
  ),
};

/** Renders a markdown string with Mantine typography styles. */
export function MarkdownContent({ children }: MarkdownContentProps) {
  return (
    <Typography style={{ maxWidth: 780, marginInline: 'auto' }}>
      <Markdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, rehypeSlug]}
        components={components}
      >
        {children}
      </Markdown>
    </Typography>
  );
}
