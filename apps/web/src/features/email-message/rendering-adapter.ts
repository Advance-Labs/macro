import { useEmailRenderCache } from '@app/lib/email-render-cache/session';
import { useUserContext } from '@core/context/user';
import { interceptMailtoLinks } from '@core/util/interceptMailtoLinks';
import { untrack } from 'solid-js';
import type { EmailRenderingContextValue } from './context/email-rendering-context';
import { fetchImagesViaPlatform, resolveCidImages } from './image-adapter';
import { emailImagePolicy } from './rendering-policy';
import { createEmailTheme } from './theme';

export function createEmailRenderingContext(): EmailRenderingContextValue {
  const cache = useEmailRenderCache();
  const user = useUserContext();
  const owner = untrack(user.userId);
  const theme = createEmailTheme();
  return {
    theme,
    canRender: () => user.isAuthenticated() === true && user.userId() === owner,
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
