import { ThrownResultError } from '@core/util/result';
import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { loadSpreadsheetDocument } from './spreadsheet-document';

const fetchContext = vi.hoisted(() => vi.fn());
vi.mock('@queries/storage/documentLoad/sync-document-context', () => ({
  fetchSyncDocumentOpenContext: fetchContext,
}));

beforeEach(() => vi.clearAllMocks());

describe('spreadsheet document loading', () => {
  it('reuses one loaded authorization and metadata result', async () => {
    const data = {
      token: 'token',
      documentMetadata: { documentId: 'sheet-1' },
    };
    fetchContext.mockResolvedValue(ok(data));
    await expect(loadSpreadsheetDocument('sheet-1')).resolves.toBe(data);
    expect(fetchContext).toHaveBeenCalledExactlyOnceWith('sheet-1');
  });

  it('maps missing documents to the access-error gate without losing error details', async () => {
    fetchContext.mockResolvedValue(
      err([
        {
          code: 'MISSING',
          message: 'Missing workbook',
          description: 'deleted',
        },
      ])
    );
    await expect(loadSpreadsheetDocument('sheet-1')).rejects.toMatchObject({
      name: 'ThrownResultError',
      errors: [
        {
          code: 'NOT_FOUND',
          message: 'Missing workbook',
          description: 'deleted',
        },
      ],
    });
  });

  it('preserves authorization errors for the access-error gate', async () => {
    fetchContext.mockResolvedValue(
      err([{ code: 'FORBIDDEN', message: 'Access denied' }])
    );
    await expect(loadSpreadsheetDocument('sheet-1')).rejects.toBeInstanceOf(
      ThrownResultError
    );
    await expect(loadSpreadsheetDocument('sheet-1')).rejects.toMatchObject({
      errors: [{ code: 'FORBIDDEN', message: 'Access denied' }],
    });
  });
});
