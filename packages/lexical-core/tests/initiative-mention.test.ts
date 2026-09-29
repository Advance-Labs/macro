// @vitest-environment jsdom
import { createHeadlessEditor } from '@lexical/headless';
import { $generateHtmlFromNodes, $generateNodesFromDOM } from '@lexical/html';
import { $getRoot, $isParagraphNode } from 'lexical';
import { describe, expect, it } from 'vitest';
import { NodeReplacements, SupportedNodeTypes } from '../node-list';
import { $isInitiativeMentionNode } from '../nodes/InitiativeMentionNode';
import { extractChannelMentionsFromMarkdown } from '../utils/markdown-mentions';
import {
  markdownToSerializedEditorStateWithIds,
  serializedEditorStateToMarkdown,
} from '../utils/markdown-state';
import { buildMentionMarkdownString } from '../utils/mentions';
import { markdownToEmbeddingText, markdownToPlainText } from '../utils/parsers';

const info = { id: 'project-1', label: 'Launch' };
const markdown = buildMentionMarkdownString({ type: 'initiative', ...info });

describe('InitiativeMentionNode', () => {
  it('round-trips Markdown and serialized state', () => {
    expect(markdown).toBe(
      '<m-initiative-mention>{"id":"project-1","label":"Launch"}</m-initiative-mention>'
    );
    const state = markdownToSerializedEditorStateWithIds(markdown);
    expect(state.root.children[0]).toMatchObject({
      children: [{ type: 'initiative-mention', ...info }],
    });
    expect(serializedEditorStateToMarkdown(state)).toBe(markdown);
  });

  it('renders readable plain text and embedding references', () => {
    expect(markdownToPlainText(markdown)).toBe('Launch');
    expect(markdownToEmbeddingText(markdown)).toBe(
      '[Launch](initiative:project-1)'
    );
    expect(
      markdownToPlainText(
        buildMentionMarkdownString({ type: 'initiative', id: 'project-1' })
      )
    ).toBe('Project');
  });

  it('is not a channel reference, which would share the project', () => {
    expect(extractChannelMentionsFromMarkdown(markdown)).toEqual([]);
  });

  it('preserves identity through HTML', () => {
    const editor = createHeadlessEditor({
      nodes: [...SupportedNodeTypes, ...NodeReplacements],
    });
    editor.setEditorState(
      editor.parseEditorState(markdownToSerializedEditorStateWithIds(markdown))
    );
    const html = editor
      .getEditorState()
      .read(() => $generateHtmlFromNodes(editor));
    expect(html).toContain('data-initiative-id="project-1"');
    editor.update(
      () => {
        const nodes = $generateNodesFromDOM(
          editor,
          new DOMParser().parseFromString(html, 'text/html')
        );
        $getRoot()
          .clear()
          .append(...nodes);
        const paragraph = $getRoot().getFirstChild();
        const node = $isParagraphNode(paragraph)
          ? paragraph.getFirstChild()
          : paragraph;
        expect($isInitiativeMentionNode(node)).toBe(true);
        if ($isInitiativeMentionNode(node)) {
          expect(node.getId()).toBe(info.id);
          expect(node.getLabel()).toBe(info.label);
        }
      },
      { discrete: true }
    );
  });
});
