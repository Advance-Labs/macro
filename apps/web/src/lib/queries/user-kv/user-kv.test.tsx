import { queryClient } from '@queries/client';
import type { UserKvEntry } from '@service-storage/generated/schemas/userKvEntry';
import { QueryClientProvider } from '@tanstack/solid-query';
import { err, ok } from 'neverthrow';
import { render } from 'solid-js/web';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { userKvKeys } from './keys';
import { usePutUserKvMutation } from './user-kv';

const putUserKv = vi.hoisted(() => vi.fn());
vi.mock('@service-storage/client', () => ({
  storageServiceClient: { putUserKv },
}));
vi.mock('@queries/client', async () => {
  const { QueryClient } = await import('@tanstack/solid-query');
  return { queryClient: new QueryClient() };
});

const entry = (key: string, value: Record<string, unknown>): UserKvEntry => ({
  namespace: 'tours',
  key,
  value,
  createdAt: '2026-09-30T00:00:00Z',
  updatedAt: '2026-09-30T00:00:00Z',
});

const tours = () =>
  queryClient.getQueryData<UserKvEntry[]>(
    userKvKeys.namespace('tours').queryKey
  );

let dispose: (() => void) | undefined;

function mountMutation() {
  let mutation!: ReturnType<typeof usePutUserKvMutation>;
  function Probe() {
    mutation = usePutUserKvMutation();
    return null;
  }
  dispose = render(
    () => (
      <QueryClientProvider client={queryClient}>
        <Probe />
      </QueryClientProvider>
    ),
    document.createElement('div')
  );
  return mutation;
}

function respondLater(): (result: unknown) => void {
  let respond!: (result: unknown) => void;
  putUserKv.mockReturnValueOnce(
    new Promise((resolve) => {
      respond = resolve;
    })
  );
  return respond;
}

beforeEach(() => {
  putUserKv.mockReset();
  queryClient.setQueryData(userKvKeys.namespace('tours').queryKey, [
    entry('mail', { status: 'completed' }),
  ]);
});

afterEach(() => {
  dispose?.();
  dispose = undefined;
  queryClient.clear();
});

describe('usePutUserKvMutation', () => {
  it('adds a new entry optimistically in key order, then keeps the saved one', async () => {
    const respond = respondLater();
    const mutation = mountMutation();

    const pending = mutation.mutateAsync({
      namespace: 'tours',
      key: 'calendar',
      value: { status: 'active', step: 1 },
    });
    await vi.waitFor(() =>
      expect(tours()?.map((e) => e.key)).toEqual(['calendar', 'mail'])
    );
    expect(tours()?.[0].value).toEqual({ status: 'active', step: 1 });

    const saved = {
      ...entry('calendar', { status: 'active', step: 1 }),
      updatedAt: '2026-09-30T00:00:05Z',
    };
    respond(ok(saved));
    await pending;
    expect(tours()?.[0]).toEqual(saved);
    expect(putUserKv).toHaveBeenCalledWith({
      namespace: 'tours',
      key: 'calendar',
      value: { status: 'active', step: 1 },
    });
  });

  it('replaces an existing entry and rolls back when the write fails', async () => {
    putUserKv.mockResolvedValueOnce(
      err([{ code: 'HTTP_ERROR', message: 'boom' }])
    );
    const mutation = mountMutation();

    await expect(
      mutation.mutateAsync({
        namespace: 'tours',
        key: 'mail',
        value: { status: 'dismissed' },
      })
    ).rejects.toBeTruthy();
    expect(tours()).toEqual([entry('mail', { status: 'completed' })]);
  });
});
