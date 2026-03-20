import { MarkdownContent } from './MarkdownContent';

export default { title: 'Components/MarkdownContent' };

const headings = '# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4';
const prose = 'A paragraph with **bold**, *italic*, and `inline code`.\n\nAnother paragraph.';
const table = '| Name | Role |\n|------|------|\n| Alice | Admin |\n| Bob | Member |';
const code = '```typescript\nfunction greet(name: string) {\n  return "Hello, " + name;\n}\n```';
const links = '[External](https://example.com) and [internal](/about) links.';

export const Headings = () => <MarkdownContent>{headings}</MarkdownContent>;
export const Prose = () => <MarkdownContent>{prose}</MarkdownContent>;
export const GFMTable = () => <MarkdownContent>{table}</MarkdownContent>;
export const CodeBlock = () => <MarkdownContent>{code}</MarkdownContent>;
export const Links = () => <MarkdownContent>{links}</MarkdownContent>;
