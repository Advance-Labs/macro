import { SERVER_HOSTS } from '@core/constant/servers';
import { fetchWithToken } from '@core/util/fetchWithToken';
import { platformFetch } from '@core/util/platformFetch';
import { err, ok } from 'neverthrow';

const host = () => `${SERVER_HOSTS['document-storage-service']}/legal`;

export function legalRequest(path: string, method = 'GET', body?: unknown) {
  return fetchWithToken<Record<string, unknown>>(host() + path, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    errorResponseHandler: async (response) => ({
      code: 'HTTP_ERROR',
      message:
        (await response.json()).message ??
        `Request failed (${response.status})`,
    }),
  });
}
export function legalDocument(id: string, completed = false) {
  return fetchWithToken<Uint8Array>(
    `${host()}/envelopes/${id}/document?completed=${completed}`
  );
}
export async function signingRequest(
  token: string,
  path: string,
  method = 'POST',
  body?: unknown
) {
  try {
    const response = await platformFetch(`${host()}/signing${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'omit',
      referrerPolicy: 'no-referrer',
    });
    if (!response.ok) return err((await response.json()).message as string);
    if (response.status === 204) return ok(undefined);
    if (response.headers.get('content-type')?.includes('application/pdf'))
      return ok(new Uint8Array(await response.arrayBuffer()));
    return ok((await response.json()) as unknown);
  } catch {
    return err('Could not connect. Check your connection and try again.');
  }
}
