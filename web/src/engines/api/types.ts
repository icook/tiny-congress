/**
 * Engine contract types
 *
 * These types define the interface between the platform (pages, router)
 * and individual engine implementations (polling, etc.).
 */

import type { Room } from '@/features/rooms';

export interface EngineMeta {
  type: string;
  displayName: string;
  description: string;
}

export interface EngineViewProps {
  room: Room;
  roomId: string;
  eligibility: { isEligible: boolean; reason?: string };
}
