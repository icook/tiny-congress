import Markdown, { type Components } from 'react-markdown';
import rehypeAutolinkHeadings from 'rehype-autolink-headings';
import rehypeRaw from 'rehype-raw';
import rehypeSlug from 'rehype-slug';
import remarkGfm from 'remark-gfm';
import { Anchor, Typography } from '@mantine/core';
import classes from './MarkdownContent.module.css';

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
    <Typography className={classes.root}>
      <Markdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, rehypeSlug, [rehypeAutolinkHeadings, { behavior: 'append' }]]}
        components={components}
      >
        {children}
      </Markdown>
    </Typography>
  );
}
