import { SlotCounter } from './SlotCounter';

export default { title: 'Endorsements/SlotCounter' };

export const Empty = () => <SlotCounter used={0} total={10} />;
export const HalfFull = () => <SlotCounter used={5} total={10} />;
export const AlmostFull = () => <SlotCounter used={9} total={10} />;
export const Full = () => <SlotCounter used={10} total={10} />;
export const WithOutOfSlot = () => <SlotCounter used={8} total={10} outOfSlot={3} />;
