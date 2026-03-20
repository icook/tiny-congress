import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { Room } from '@/engines/polling/api';
import { DeviceContext, type DeviceContextValue } from '@/providers/DeviceProvider';
import { Navbar } from './Navbar';

const noOp = () => {
  // Intentionally empty for storybook
};

const fakeRooms: Room[] = [
  {
    id: 'room-1',
    name: 'General Assembly',
    description: 'Main deliberation room',
    eligibility_topic: 'membership',
    engine_type: 'ranked_choice',
    engine_config: {},
    status: 'active',
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: 'room-2',
    name: 'Budget Committee',
    description: null,
    eligibility_topic: 'membership',
    engine_type: 'approval',
    engine_config: {},
    status: 'active',
    created_at: '2026-01-15T00:00:00Z',
  },
];

const guestDevice: DeviceContextValue = {
  deviceKid: null,
  privateKey: null,
  username: null,
  isLoading: false,
  setDevice: noOp,
  clearDevice: noOp,
};

const authedDevice: DeviceContextValue = {
  deviceKid: 'AAAAAAAAAAAAAAAAAAAAAA',
  privateKey: null,
  username: 'alice',
  isLoading: false,
  setDevice: noOp,
  clearDevice: noOp,
};

function SeedRooms({ children }: { children: React.ReactNode }) {
  const queryClient = useQueryClient();
  useEffect(() => {
    queryClient.setQueryData(['rooms'], fakeRooms);
  }, [queryClient]);
  return <>{children}</>;
}

export default { title: 'Components/Navbar' };

export const Guest = () => (
  <DeviceContext.Provider value={guestDevice}>
    <SeedRooms>
      <div style={{ width: 260, height: '100vh' }}>
        <Navbar onNavigate={noOp} />
      </div>
    </SeedRooms>
  </DeviceContext.Provider>
);

export const Authenticated = () => (
  <DeviceContext.Provider value={authedDevice}>
    <SeedRooms>
      <div style={{ width: 260, height: '100vh' }}>
        <Navbar onNavigate={noOp} />
      </div>
    </SeedRooms>
  </DeviceContext.Provider>
);

export const AuthenticatedNoRooms = () => (
  <DeviceContext.Provider value={authedDevice}>
    <div style={{ width: 260, height: '100vh' }}>
      <Navbar onNavigate={noOp} />
    </div>
  </DeviceContext.Provider>
);
