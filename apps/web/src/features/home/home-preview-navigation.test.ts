import type { PreviewBlockTarget } from '@components/app/previewTarget';
import { expect, it, vi } from 'vitest';

vi.mock('@service-storage/websocket', () => ({
  storageWS: { reconnectIfDisconnected: vi.fn() },
  createWebSocketJob: vi.fn(),
}));
vi.mock('@service-connection/websocket', () => ({
  ws: { addEventListener: vi.fn(), send: vi.fn() },
  state: () => 'closed',
  createConnectionWebsocketEffect: vi.fn(),
}));

import {
  homeDetailSearch,
  homePreviewTargetNavigation,
} from './home-preview-navigation';

it('encodes a fresh comment request and clears old feature locations', () => {
  const target = {
    blockType: 'md' as const,
    aliasContext: undefined,
    blockId: 'document-1',
    params: { comment_id: 'comment-1' },
  };
  const first = homePreviewTargetNavigation(target);
  const repeated = homePreviewTargetNavigation(target);
  expect(first.search['markdown-detail']).toMatchObject({
    documentId: ['document-1'],
    commentId: ['comment-1'],
    seek: [expect.any(String)],
  });
  expect(first.search['markdown-detail']?.seek).not.toEqual(
    repeated.search['markdown-detail']?.seek
  );
  const next = homePreviewTargetNavigation({
    blockType: 'channel',
    aliasContext: undefined,
    blockId: 'channel-1',
    params: { channel_message_id: 'message-1' },
  });
  expect(next.search.channels).toMatchObject({
    messageId: ['message-1'],
    seek: [expect.any(String)],
  });
  expect(next.search['markdown-detail']).toBeUndefined();
  expect(next.search['pdf-detail']).toBeUndefined();
  expect(next.search['canvas-detail']).toBeUndefined();
  expect(next.search['chat-detail']).toBeUndefined();
  expect(next.search['email-detail']).toBeUndefined();
  expect(homeDetailSearch().channels).toBeUndefined();
});

it('delivers chat requests through chat detail', () => {
  expect(
    homePreviewTargetNavigation({
      blockType: 'chat',
      aliasContext: undefined,
      blockId: 'chat-1',
      params: { message_id: 'message-1' },
    }).search['chat-detail']
  ).toMatchObject({
    chatId: ['chat-1'],
    messageId: ['message-1'],
    seek: [expect.any(String)],
  });
});

it.each<{
  blockType: PreviewBlockTarget['blockType'];
  params: Record<string, string>;
  namespace: string;
  expected: Record<string, string[]>;
}>([
  {
    blockType: 'md',
    params: { node_id: 'node-1', comment_id: 'comment-1' },
    namespace: 'markdown-detail',
    expected: { nodeId: ['node-1'], commentId: ['comment-1'] },
  },
  {
    blockType: 'pdf',
    params: {
      pdf_page_number: '3',
      pdf_page_y: '0.4',
      pdf_page_x: '0.2',
      pdf_width: '0.1',
      pdf_height: '0.3',
    },
    namespace: 'pdf-detail',
    expected: {
      pageNumber: ['3'],
      yPos: ['0.4'],
      x: ['0.2'],
      width: ['0.1'],
      height: ['0.3'],
    },
  },
  {
    blockType: 'pdf',
    params: {
      pdf_search_page: '0',
      pdf_search_highlight_terms: '["needle"]',
      pdf_search_snippet: 'snippet',
      pdf_search_raw_query: 'query',
    },
    namespace: 'pdf-detail',
    expected: {
      page: ['0'],
      highlightTerms: ['needle'],
      snippet: ['snippet'],
      query: ['query'],
    },
  },
  {
    blockType: 'canvas',
    params: { canvas_x: '12', canvas_y: '25', canvas_scale: '2' },
    namespace: 'canvas-detail',
    expected: { x: ['12'], y: ['25'], scale: ['2'] },
  },
  {
    blockType: 'email',
    params: { email_message_id: 'message-1' },
    namespace: 'email-detail',
    expected: { messageId: ['message-1'] },
  },
])('encodes fresh $blockType locations using feature codecs', (test) => {
  const target: PreviewBlockTarget = {
    blockType: test.blockType,
    blockId: 'item-1',
    aliasContext: undefined,
    params: test.params,
  };
  const first = homePreviewTargetNavigation(target);
  const repeated = homePreviewTargetNavigation(target);
  expect(first.search[test.namespace]).toMatchObject(test.expected);
  expect(first.search[test.namespace]?.seek).not.toEqual(
    repeated.search[test.namespace]?.seek
  );
});
