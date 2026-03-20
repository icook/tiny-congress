import { ErrorFallback } from './ErrorFallback';

export default { title: 'Components/ErrorFallback' };

export const Default = () => <ErrorFallback />;
export const WithContext = () => <ErrorFallback context="Router" />;
export const WithError = () => (
  <ErrorFallback error={new Error('Something went wrong\n    at Component (app.tsx:42)')} />
);
export const WithErrorAndContext = () => (
  <ErrorFallback context="Dashboard" error={new Error('Network timeout')} />
);
