import type React from 'react';
import type { EngineViewProps } from './api';

export const engineMap: Record<
  string,
  () => Promise<{ EngineView: React.ComponentType<EngineViewProps> }>
> = {
  polling: () => import('./polling'),
};
