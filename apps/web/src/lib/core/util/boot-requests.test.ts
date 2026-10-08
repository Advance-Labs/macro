import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const URL_PATH = 'https://auth.example.com/user/legacy_user_permissions';
const cookieGet = { credentials: 'include' as const, headers: {} };

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.resetModules();
  fetchMock = vi.fn(async () => new Response('{}'));
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

async function load() {
  return await import('./boot-requests');
}

describe('boot requests', () => {
  it('starts a cookie GET and hands its response to the first matching request only', async () => {
    const { startBootRequest, takeBootResponse } = await load();
    startBootRequest(URL_PATH);
    expect(fetchMock).toHaveBeenCalledWith(URL_PATH, {
      credentials: 'include',
    });

    expect(takeBootResponse(URL_PATH, cookieGet)).toBeInstanceOf(Promise);
    expect(takeBootResponse(URL_PATH, cookieGet)).toBeUndefined();
  });

  it.each([
    ['another method', { ...cookieGet, method: 'POST' }],
    ['no cookies', { headers: {} }],
    [
      'its own authorization',
      { ...cookieGet, headers: { Authorization: 'Bearer t' } },
    ],
  ])('leaves a request with %s to the network', async (_, init) => {
    const { startBootRequest, takeBootResponse } = await load();
    startBootRequest(URL_PATH);

    expect(takeBootResponse(URL_PATH, init)).toBeUndefined();
    expect(takeBootResponse(URL_PATH, cookieGet)).toBeInstanceOf(Promise);
  });

  it('drops a response that is too old to trust', async () => {
    vi.useFakeTimers();
    const { startBootRequest, takeBootResponse } = await load();
    startBootRequest(URL_PATH);
    vi.advanceTimersByTime(15_001);

    expect(takeBootResponse(URL_PATH, cookieGet)).toBeUndefined();
  });

  it('matches only the URL it started', async () => {
    const { startBootRequest, takeBootResponse } = await load();
    startBootRequest(URL_PATH);

    expect(
      takeBootResponse('https://auth.example.com/user/me', cookieGet)
    ).toBeUndefined();
  });
});
