import type { MimeType } from './block-registry';

export const SUPPORTED_VIDEO_MIMES = {
  mp4: 'video/mp4',
  mkv: 'video/x-matroska',
  webm: 'video/webm',
  avi: 'video/x-msvideo',
  mov: 'video/quicktime',
  wmv: 'video/x-ms-wmv',
  mpg: 'video/mpeg',
  mpeg: 'video/mpeg',
  m4v: 'video/mp4',
  flv: 'video/x-flv',
  f4v: 'video/mp4',
  threegp: 'video/3gpp',
} as const satisfies Record<string, MimeType>;

export const VIDEO_MIMES = SUPPORTED_VIDEO_MIMES;
