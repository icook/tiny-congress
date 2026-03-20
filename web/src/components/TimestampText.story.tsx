import { TimestampText } from './TimestampText';

export default { title: 'Components/TimestampText' };

const recent = new Date(Date.now() - 5 * 60 * 1000).toISOString();
const past = '2023-06-15T14:30:00Z';

export const LocalMode = () => <TimestampText value={past} defaultMode="local" />;
export const UtcMode = () => <TimestampText value={past} defaultMode="utc" />;
export const RelativeMode = () => <TimestampText value={recent} defaultMode="relative" />;
export const FarPast = () => <TimestampText value="2020-01-01T00:00:00Z" defaultMode="relative" />;
export const SmallDimmed = () => <TimestampText value={past} size="xs" c="dimmed" />;
