import { isVideoPlaybackEnabled } from '@block-video/core/video';
import { ENABLE_VIDEO_BLOCK } from '@core/constant/featureFlags';
import {
  type FileDocumentData,
  getFileDocumentUrl,
  loadFileDocumentData,
} from './file-document';

export type VideoDocumentData = FileDocumentData & {
  videoUrl?: string;
};

export async function loadVideoDocument(
  documentId: string
): Promise<VideoDocumentData> {
  const data = await loadFileDocumentData(documentId);
  if (
    !ENABLE_VIDEO_BLOCK ||
    !isVideoPlaybackEnabled(data.documentMetadata.fileType)
  )
    return data;

  const videoUrl = await getFileDocumentUrl({
    documentId,
    documentVersionId: data.documentMetadata.documentVersionId,
  });
  return { ...data, videoUrl };
}
