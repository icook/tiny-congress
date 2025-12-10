import { AuthProvider } from '../../auth/AuthProvider';
import { Welcome } from './Welcome';

export default {
  title: 'Welcome',
};

export const Usage = () => (
  <AuthProvider>
    <Welcome />
  </AuthProvider>
);
