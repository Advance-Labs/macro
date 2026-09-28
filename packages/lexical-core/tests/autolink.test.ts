import { createHeadlessEditor } from '@lexical/headless';
import { $isLinkNode } from '@lexical/link';
import { $convertFromMarkdownString } from '@lexical/markdown';
import { $getRoot } from 'lexical';
import { describe, expect, it } from 'vitest';
import { NodeReplacements, SupportedNodeTypes } from '../node-list';
import { ALL_TRANSFORMERS, INTERNAL_TRANSFORMERS } from '../transformers';
import { composeAgentChatReply } from '../utils/agent-chat-reply';
import {
  markdownToSerializedEditorStateWithIds,
  serializedEditorStateToMarkdown,
} from '../utils/markdown-state';

function parse(markdown: string, transformers = ALL_TRANSFORMERS) {
  const editor = createHeadlessEditor({
    nodes: [...SupportedNodeTypes, ...NodeReplacements],
  });
  editor.update(() => $convertFromMarkdownString(markdown, transformers), {
    discrete: true,
  });
  return editor.getEditorState();
}

/** Every link in the parsed markdown, as `[url, text, node type]`. */
function links(markdown: string, transformers = ALL_TRANSFORMERS) {
  return parse(markdown, transformers).read(() =>
    $getRoot()
      .getAllTextNodes()
      .map((text) => [text.getParent(), text.getTextContent()] as const)
      .filter(([parent]) => $isLinkNode(parent))
      .map(([parent, text]) => [parent!.getURL(), text, parent!.getType()])
  );
}

/** The plain text of the parsed markdown, to show nothing was dropped. */
function text(markdown: string) {
  return parse(markdown).read(() => $getRoot().getTextContent());
}

describe('bare URLs in markdown', () => {
  it('links a URL written as plain text', () => {
    const markdown = 'see https://macro.com/app/channels/abc ok';
    expect(links(markdown)).toEqual([
      [
        'https://macro.com/app/channels/abc',
        'https://macro.com/app/channels/abc',
        'autolink',
      ],
    ]);
    expect(text(markdown)).toBe(markdown);
  });

  it('leaves sentence punctuation outside the link', () => {
    expect(links('Opened https://macro.com/app, then waited.')).toEqual([
      ['https://macro.com/app', 'https://macro.com/app', 'autolink'],
    ]);
    expect(links('Details (https://macro.com/app/docs/1).')).toEqual([
      [
        'https://macro.com/app/docs/1',
        'https://macro.com/app/docs/1',
        'autolink',
      ],
    ]);
    expect(text('Opened https://macro.com/app, then waited.')).toBe(
      'Opened https://macro.com/app, then waited.'
    );
  });

  it('links every URL in a paragraph', () => {
    expect(links('https://macro.com/one and https://macro.com/two')).toEqual([
      ['https://macro.com/one', 'https://macro.com/one', 'autolink'],
      ['https://macro.com/two', 'https://macro.com/two', 'autolink'],
    ]);
  });

  it('links a mailto URL', () => {
    expect(links('mail wolf: mailto:wolf@macro.com')).toEqual([
      ['mailto:wolf@macro.com', 'mailto:wolf@macro.com', 'autolink'],
    ]);
  });

  it('leaves a scheme links must not open as text', () => {
    expect(links('run file:///etc/passwd')).toEqual([]);
    expect(text('run file:///etc/passwd')).toBe('run file:///etc/passwd');
  });

  it('leaves bare hosts alone so file names stay file names', () => {
    expect(links('Edited crates/prompt/src/lib.rs and macro.com')).toEqual([]);
  });

  it('keeps markdown and internal links as they were written', () => {
    expect(links('see [the channel](https://macro.com/app) ok')).toEqual([
      ['https://macro.com/app', 'the channel', 'link'],
    ]);
    expect(
      links(
        '<m-link>{"url":"https://macro.com/app","text":"https://macro.com/app","title":""}</m-link>'
      )
    ).toEqual([['https://macro.com/app', 'https://macro.com/app', 'link']]);
  });

  it('leaves URLs inside code alone', () => {
    expect(links('run `curl https://macro.com/app`')).toEqual([]);
    expect(links('```sh\ncurl https://macro.com/app\n```')).toEqual([]);
  });

  it('stops the link at the node tag beside it', () => {
    const markdown =
      'ask https://macro.com/app<m-user-mention>{"userId":"macro|wolf@macro.com","email":"wolf@macro.com","displayName":"Wolf"}</m-user-mention>';
    expect(links(markdown)).toEqual([
      ['https://macro.com/app', 'https://macro.com/app', 'autolink'],
    ]);
  });

  it('links an angle-bracketed URL without the brackets', () => {
    expect(links('see <https://macro.com/app> ok')).toEqual([
      ['https://macro.com/app', 'https://macro.com/app', 'autolink'],
    ]);
  });

  it('leaves a URL inside another node payload alone', () => {
    const markdown =
      '<m-reply-target>{"parent":{"type":"channel","id":"c1"},"targetMessageId":"m1","targetThreadId":"t1","displayText":"look at https://macro.com/app","senderId":"macro|wolf@macro.com"}</m-reply-target>';
    expect(links(markdown)).toEqual([]);
  });

  it('links a URL in an agent reply, alongside its session link', () => {
    const markdown = composeAgentChatReply({
      sessionId: '00000000-0000-0000-0000-00000000000a',
      body: {
        kind: 'markdown',
        markdown: 'Posted the summary in https://macro.com/app/channels/abc.',
      },
    });
    expect(links(markdown)).toEqual([
      [
        'https://macro.com/app/channels/abc',
        'https://macro.com/app/channels/abc',
        'autolink',
      ],
    ]);
  });

  it('links bare URLs through the internal transformers too', () => {
    expect(links('see https://macro.com/app', INTERNAL_TRANSFORMERS)).toEqual([
      ['https://macro.com/app', 'https://macro.com/app', 'autolink'],
    ]);
  });

  it('writes the link back as an internal link node', () => {
    const state = markdownToSerializedEditorStateWithIds(
      'see https://macro.com/app'
    );
    expect(serializedEditorStateToMarkdown(state)).toBe(
      'see <m-link>{"url":"https://macro.com/app","text":"https://macro.com/app","title":""}</m-link>'
    );
  });
});
