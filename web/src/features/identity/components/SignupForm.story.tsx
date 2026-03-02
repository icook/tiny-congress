import { SignupForm } from './SignupForm';

export default {
  title: 'Identity/SignupForm',
};

const noOp = () => {
  // Intentionally empty for storybook
};
const preventSubmit = (e: React.FormEvent) => {
  e.preventDefault();
};

// Default - empty form ready for input
export const Default = () => (
  <SignupForm
    username=""
    password=""
    onUsernameChange={noOp}
    onPasswordChange={noOp}
    onSubmit={preventSubmit}
    isLoading={false}
  />
);

// Filled - form with username and password entered
export const Filled = () => (
  <SignupForm
    username="alice"
    password="my-secret-password"
    onUsernameChange={noOp}
    onPasswordChange={noOp}
    onSubmit={preventSubmit}
    isLoading={false}
  />
);

// Loading state during key generation
export const GeneratingKeys = () => (
  <SignupForm
    username="alice"
    password="my-secret-password"
    onUsernameChange={noOp}
    onPasswordChange={noOp}
    onSubmit={preventSubmit}
    isLoading
    loadingText="Generating keys and encrypting backup..."
  />
);

// Loading state during API submission
export const Submitting = () => (
  <SignupForm
    username="alice"
    password="my-secret-password"
    onUsernameChange={noOp}
    onPasswordChange={noOp}
    onSubmit={preventSubmit}
    isLoading
  />
);

// Error state
export const Error = () => (
  <SignupForm
    username="alice"
    password="my-secret-password"
    onUsernameChange={noOp}
    onPasswordChange={noOp}
    onSubmit={preventSubmit}
    isLoading={false}
    error="Username already taken"
  />
);

// Success state with account details
export const Success = () => (
  <SignupForm
    username=""
    password=""
    onUsernameChange={noOp}
    onPasswordChange={noOp}
    onSubmit={preventSubmit}
    isLoading={false}
    successData={{
      account_id: 'acc_abc123def456',
      root_kid: 'kid_xyz789uvw012',
      device_kid: 'dev_abc123def456',
    }}
  />
);
