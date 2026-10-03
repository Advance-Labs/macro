import { useLocation } from '@solidjs/router';
import { Show } from 'solid-js';
import { signingSource } from './queries/source';
import { SigningView } from './views/signing-view';
export function Signing() {
  // The email capability stays in the fragment, never in server URL logs or referrers.
  const location = useLocation();
  return (
    <Show
      keyed
      when={location.hash.slice(1)}
      fallback={
        <div class="p-16 text-center">
          This signing link could not be opened.
        </div>
      }
    >
      {(token) => <SigningView source={signingSource(token)} />}
    </Show>
  );
}
