import { DocsList, type DocEntry } from './DocsList';

export default { title: 'Components/DocsList' };

const docs: DocEntry[] = [
  { title: 'Domain Model', description: 'Core entities and trust boundaries', path: '/docs' },
  { title: 'Architecture', description: 'System design and data flow', path: '/dev/architecture' },
  {
    title: 'Domain Model Reference',
    description: 'Detailed entity schemas and validation rules',
    path: '/dev/domain-model',
  },
];

export const Default = () => <DocsList docs={docs} />;
export const SingleEntry = () => <DocsList docs={docs.slice(0, 1)} />;
export const Empty = () => <DocsList docs={[]} />;
