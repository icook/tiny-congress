import { useEffect, useState } from 'react';
import type { Poll } from '../api';

export interface CountdownState {
  secondsLeft: number | null;
  isExpired: boolean;
}

export function usePollCountdown(poll?: Poll): CountdownState {
  const [secondsLeft, setSecondsLeft] = useState<number | null>(null);

  const closesAt = poll?.closes_at ?? null;

  useEffect(() => {
    if (!closesAt) {
      setSecondsLeft(null);
      return;
    }

    const update = () => {
      const ms = new Date(closesAt).getTime() - Date.now();
      setSecondsLeft(Math.max(0, Math.floor(ms / 1000)));
    };

    update();
    const id = setInterval(update, 1000);
    return () => {
      clearInterval(id);
    };
  }, [closesAt]);

  return {
    secondsLeft,
    isExpired: secondsLeft !== null && secondsLeft <= 0,
  };
}
