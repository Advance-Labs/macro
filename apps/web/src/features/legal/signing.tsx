import { signingSource } from './queries/source';
import { SigningView } from './views/signing-view';
export function Signing() {
  // The email capability stays in the fragment, never in server URL logs or referrers.
  const token = window.location.hash.slice(1);
  return <SigningView source={signingSource(token)} />;
}
