import { useEffect } from 'react';
import { Outlet, useNavigate } from '@tanstack/react-router';
import { useDevice } from '@/providers/DeviceProvider';

/** Reactive fallback for auth-required routes — redirects on logout while on a protected page. */
export function AuthRequiredOutlet() {
  const { deviceKid } = useDevice();
  const navigate = useNavigate();

  useEffect(() => {
    if (!deviceKid) {
      void navigate({ to: '/login' });
    }
  }, [deviceKid, navigate]);

  if (!deviceKid) {
    return null;
  }

  return <Outlet />;
}
