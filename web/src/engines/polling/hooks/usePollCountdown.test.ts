import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Poll } from '../api';
import { usePollCountdown } from './usePollCountdown';

function makePoll(closesAt: string | null): Poll {
  return {
    id: '1',
    room_id: 'r1',
    question: 'Q?',
    description: null,
    status: 'active',
    created_at: new Date().toISOString(),
    closes_at: closesAt,
  };
}

describe('usePollCountdown', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns null when poll is undefined', () => {
    const { result } = renderHook(() => usePollCountdown());
    expect(result.current.secondsLeft).toBeNull();
    expect(result.current.isExpired).toBe(false);
  });

  it('returns null when poll has no closes_at', () => {
    const { result } = renderHook(() => usePollCountdown(makePoll(null)));
    expect(result.current.secondsLeft).toBeNull();
    expect(result.current.isExpired).toBe(false);
  });

  it('returns seconds remaining when closes_at is in the future', () => {
    const future = new Date(Date.now() + 60_000).toISOString();
    const { result } = renderHook(() => usePollCountdown(makePoll(future)));
    expect(result.current.secondsLeft).toBe(60);
    expect(result.current.isExpired).toBe(false);
  });

  it('returns isExpired true when closes_at is in the past', () => {
    const past = new Date(Date.now() - 1000).toISOString();
    const { result } = renderHook(() => usePollCountdown(makePoll(past)));
    expect(result.current.secondsLeft).toBe(0);
    expect(result.current.isExpired).toBe(true);
  });

  it('ticks down over time', () => {
    const future = new Date(Date.now() + 10_000).toISOString();
    const { result } = renderHook(() => usePollCountdown(makePoll(future)));

    act(() => {
      vi.advanceTimersByTime(3_000);
    });

    expect(result.current.secondsLeft).toBe(7);
  });
});
