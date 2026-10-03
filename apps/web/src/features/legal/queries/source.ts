import {
  legalDocument,
  legalRequest,
  signingRequest,
} from '@app/lib/service-clients/service-legal/client';
import type { Result } from 'neverthrow';
import { z } from 'zod';
import type { LegalSource, SigningSource } from '../context/legal-source';
import { envelopeSchema, sessionSchema } from '../core/models';

// This boundary translates transport errors into the source contract consumed by
// the reactive workflow. Domain data is decoded before it enters feature state.
function resultValue<T, E>(result: Result<T, E>): T {
  return result.match(
    (value) => value,
    (error) => {
      throw new Error(
        typeof error === 'string'
          ? error
          : Array.isArray(error)
            ? error.map((e) => e.message).join(' ')
            : 'Request failed.'
      );
    }
  );
}
async function fileBase64(file: File) {
  if (file.size > 10 * 1024 * 1024)
    throw new Error('Choose a PDF up to 10 MB.');
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = '';
  for (let i = 0; i < bytes.length; i += 8192)
    binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(binary);
}
export const legalSource: LegalSource = {
  async list() {
    return z
      .array(envelopeSchema)
      .parse(resultValue(await legalRequest('/envelopes')));
  },
  async create(file, title) {
    return envelopeSchema.parse(
      resultValue(
        await legalRequest('/envelopes', 'POST', {
          title,
          filename: file.name,
          documentBase64: await fileBase64(file),
        })
      )
    );
  },
  async update(id, draft) {
    return envelopeSchema.parse(
      resultValue(await legalRequest(`/envelopes/${id}`, 'PUT', draft))
    );
  },
  async send(id, revision) {
    return envelopeSchema.parse(
      resultValue(
        await legalRequest(`/envelopes/${id}/send`, 'POST', { revision })
      )
    );
  },
  async resend(id) {
    return envelopeSchema.parse(
      resultValue(await legalRequest(`/envelopes/${id}/resend`, 'POST', {}))
    );
  },
  async void(id, revision, reason) {
    return envelopeSchema.parse(
      resultValue(
        await legalRequest(`/envelopes/${id}/void`, 'POST', {
          revision,
          reason,
        })
      )
    );
  },
  async document(id, completed) {
    return resultValue(await legalDocument(id, completed));
  },
};
export function signingSource(token: string): SigningSource {
  return {
    async session() {
      return sessionSchema.parse(
        resultValue(await signingRequest(token, '/session'))
      );
    },
    async document(completed) {
      const bytes = resultValue(
        await signingRequest(token, `/document?completed=${!!completed}`, 'GET')
      );
      if (!(bytes instanceof Uint8Array))
        throw new Error('Could not load the PDF.');
      return bytes;
    },
    async sign(body) {
      return sessionSchema.parse(
        resultValue(await signingRequest(token, '/sign', 'POST', body))
      );
    },
    async decline(revision, reason) {
      resultValue(
        await signingRequest(token, '/decline', 'POST', { revision, reason })
      );
    },
  };
}
