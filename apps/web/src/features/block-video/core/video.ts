import type { SUPPORTED_VIDEO_MIMES } from '@app/lib/constants/video-file-types';
import type { DocumentMetadataFileType } from '@service-storage/generated/schemas/documentMetadataFileType';

export { VIDEO_MIMES } from '@app/lib/constants/video-file-types';

type SupportedVideoFileType = keyof typeof SUPPORTED_VIDEO_MIMES;

export const PLAYBACK_ENABLED_MIMES = {
  mp4: true,
  mkv: true,
  webm: true,
  avi: true,
  mov: true,
  wmv: true,
  mpg: true,
  mpeg: true,
  m4v: true,
  flv: true,
  f4v: true,
  threegp: true,
} as const satisfies Record<SupportedVideoFileType, boolean>;

export function isVideoPlaybackEnabled(
  fileType: DocumentMetadataFileType | undefined
): fileType is SupportedVideoFileType {
  return (
    fileType != null &&
    fileType in PLAYBACK_ENABLED_MIMES &&
    PLAYBACK_ENABLED_MIMES[fileType as SupportedVideoFileType]
  );
}
