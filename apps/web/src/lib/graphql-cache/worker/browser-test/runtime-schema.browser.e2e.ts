import { expect, test } from '@playwright/test';

test('OTA email fields populate on fetch and survive OPFS reopen without a backfill', async ({
  page,
}) => {
  await page.goto('runtime-schema.html');
  const outcome = await page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const worker = new Worker('./runtime-schema.worker.ts', {
          type: 'module',
        });
        worker.onerror = (event) => {
          worker.terminate();
          reject(new Error(event.message));
        };
        worker.onmessage = (event) => {
          worker.terminate();
          resolve(event.data);
        };
        worker.postMessage('run');
      })
  );
  const hit = {
    kind: 'hit',
    data: {
      user: {
        id: 'viewer',
        emailThread: {
          id: 'thread',
          messages: [
            {
              id: 'message',
              bodyText: 'cached body',
              otaCalendarInvitations: [{ uid: 'meeting' }],
            },
          ],
        },
      },
    },
  };
  expect(outcome).toEqual({
    result: {
      rejectedBeforeUpdate: true,
      beforeFetch: { kind: 'miss' },
      reopened: hit,
      incompatibleRejected: true,
      afterRejection: hit,
      afterReset: hit,
      sameGeneration: true,
    },
  });
});
