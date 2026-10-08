/**
 * Cookie-authenticated GETs that `src/boot.ts` starts while the app bundle
 * downloads, so the first app request for the same URL finds its response in
 * flight. Responses live only in memory, are handed out once, and expire.
 */
type BootRequest = { started: number; response: Promise<Response> };

/** Older than this, a boot response is dropped and the app requests afresh. */
const MAX_AGE_MS = 15_000;

const requests = new Map<string, BootRequest>();

function keyOf(url: string): string {
  return new URL(url, window.location.href).href;
}

export function startBootRequest(url: string): void {
  const response = fetch(url, { credentials: 'include' });
  // An unclaimed failure must not surface as an unhandled rejection.
  response.catch(() => {});
  requests.set(keyOf(url), { started: Date.now(), response });
}

/**
 * The boot response for a cookie GET to `url`, at most once. Requests with
 * their own credentials or headers don't match what the boot request sent.
 */
export function takeBootResponse(
  url: string,
  init: RequestInit & { headers: Record<string, string> }
): Promise<Response> | undefined {
  if (requests.size === 0) return;
  const method = (init.method ?? 'GET').toUpperCase();
  if (method !== 'GET' || init.credentials !== 'include') return;
  if ('Authorization' in init.headers) return;

  const key = keyOf(url);
  const request = requests.get(key);
  if (!request) return;
  requests.delete(key);
  if (Date.now() - request.started > MAX_AGE_MS) return;

  return request.response;
}
