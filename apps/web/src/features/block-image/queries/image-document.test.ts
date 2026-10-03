import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { loadImageDocument } from './image-document';

const mocks = vi.hoisted(() => ({
  loadDocument: vi.fn(),
  fetchBinary: vi.fn(),
}));

vi.mock('@queries/storage/binary-document', () => ({
  fetchBinaryDocumentData: mocks.loadDocument,
}));
vi.mock('@service-storage/util/fetchBinary', () => ({
  fetchBinary: mocks.fetchBinary,
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe('image document loading', () => {
  it('keeps metadata and binary fetching owned by the existing loader', async () => {
    const metadata = {
      documentId: 'image-1',
      documentName: 'photo',
      fileType: 'png',
    };
    const blob = new Blob(['image']);
    mocks.loadDocument.mockResolvedValue(
      ok({
        documentMetadata: metadata,
        userAccessLevel: 'view',
        blobUrl: 'https://fixture.invalid/image-1',
      })
    );
    mocks.fetchBinary.mockResolvedValue(ok(blob));

    const result = await loadImageDocument('image-1');
    expect(result.documentMetadata).toBe(metadata);
    expect(result.file).toBe(blob);
    expect(result.userAccessLevel).toBe('view');
    expect(mocks.loadDocument).toHaveBeenCalledExactlyOnceWith('image-1');
    expect(mocks.fetchBinary).toHaveBeenCalledExactlyOnceWith(
      'https://fixture.invalid/image-1',
      'blob'
    );
  });

  it('does not request a binary after authorization fails', async () => {
    mocks.loadDocument.mockResolvedValue(
      err([{ code: 'UNAUTHORIZED', message: 'No access' }])
    );
    await expect(loadImageDocument('image-1')).rejects.toThrow('No access');
    expect(mocks.loadDocument).toHaveBeenCalledOnce();
    expect(mocks.fetchBinary).not.toHaveBeenCalled();
  });
});
