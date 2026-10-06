import { useEmailRenderCache } from '@app/lib/email-render-cache/session';
import { useUserContext } from '@core/context/user';
import { interceptMailtoLinks } from '@core/util/interceptMailtoLinks';
import { createMemo } from 'solid-js';
import type { EmailRenderingContextValue } from './context/email-rendering-context';
import { fetchImagesViaPlatform, resolveCidImages } from './image-adapter';
import { emailImagePolicy } from './rendering-policy';
import { createEmailTheme } from './theme';

export function createEmailRenderingContext(): EmailRenderingContextValue {
  const cache = useEmailRenderCache();
  const user = useUserContext();
  // The first viewer this surface renders for. A cold start may mount before
  // user info loads, so adopt the first known id instead of snapshotting.
  const owner = createMemo<string | undefined>(
    (first) => first ?? (user.userId() || undefined)
  );
  const theme = createEmailTheme();
  return {
    theme,
    // Revoke only on definitive sign-out or an account switch. An unknown auth
    // state, such as an offline cold start, keeps rendering cached mail.
    canRender: () => {
      if (user.isAuthenticated() === false) return false;
      const id = user.userId();
      return !id || id === owner();
    },
    get preparation() {
      return cache();
    },
    images: emailImagePolicy,
    prepareLinks: interceptMailtoLinks,
    async resolveImages(root, attachments, lifetime) {
      const blobUrls: string[] = [];
      const isDisposed = () => lifetime.signal.aborted;
      lifetime.onDispose(() => {
        for (const url of blobUrls) URL.revokeObjectURL(url);
      });
      if (isDisposed()) return;
      resolveCidImages(root, attachments);
      if (isDisposed()) return;
      await fetchImagesViaPlatform(root, blobUrls, isDisposed);
    },
  };
}
