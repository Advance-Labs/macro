import { SPREADSHEET_COMMENT_PARAMS } from '@app/features/block-spreadsheet/core/spreadsheet-comments';
import { URL_PARAMS as MARKDOWN_URL_PARAMS } from '@block-md/constants';
import { URL_PARAMS as PDF_URL_PARAMS } from '@block-pdf/constants';

/**
 * The legacy query keys a link to a document carries — the comment,
 * annotation, node, or location a notification or copied link names. Drive's
 * detail routes and the block routes touch devices stay on open the same
 * documents, so both keep these keys through a navigation.
 *
 * `blockType` is the resolved block, not the URL segment: task, skill, and
 * snippet documents are markdown.
 */
export function documentLinkParams(blockType: string): readonly string[] {
  switch (blockType) {
    case 'md':
      return Object.values(MARKDOWN_URL_PARAMS);
    case 'pdf':
      return Object.values(PDF_URL_PARAMS);
    case 'spreadsheet':
      return Object.values(SPREADSHEET_COMMENT_PARAMS);
    default:
      return [];
  }
}
