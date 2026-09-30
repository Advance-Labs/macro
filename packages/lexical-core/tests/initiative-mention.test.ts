import { describe, expect, it } from 'vitest';
import { extractChannelMentionsFromMarkdown } from '../utils/markdown-mentions';
import {
  markdownToSerializedEditorStateWithIds,
  serializedEditorStateToMarkdown,
} from '../utils/markdown-state';
import { buildMentionMarkdownString } from '../utils/mentions';
import { markdownToEmbeddingText, markdownToPlainText } from '../utils/parsers';

// A task project is a document mention with the `initiative` block name.
const project = buildMentionMarkdownString({
  type: 'document',
  documentId: 'project-1',
  documentName: 'Launch',
  blockName: 'initiative',
});

describe('project (initiative) document mentions', () => {
  it('round-trips as a document mention', () => {
    const state = markdownToSerializedEditorStateWithIds(project);
    expect(state.root.children[0]).toMatchObject({
      children: [
        {
          type: 'document-mention',
          documentId: 'project-1',
          blockName: 'initiative',
        },
      ],
    });
    const exported = serializedEditorStateToMarkdown(state);
    expect(exported).toContain('"documentId":"project-1"');
    expect(exported).toContain('"blockName":"initiative"');
  });

  it('reads as the project name and embeds as an initiative reference', () => {
    expect(markdownToPlainText(project)).toBe('Launch');
    expect(markdownToEmbeddingText(project)).toBe(
      '[Launch](initiative:project-1)'
    );
  });

  it('is not a channel reference, which would share the project', () => {
    const doc = buildMentionMarkdownString({
      type: 'document',
      documentId: 'doc-1',
      documentName: 'Spec',
      blockName: 'md',
    });
    expect(extractChannelMentionsFromMarkdown(`${project} ${doc}`)).toEqual([
      { entityType: 'document', entityId: 'doc-1' },
    ]);
  });
});
