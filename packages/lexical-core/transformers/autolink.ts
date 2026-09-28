import { $createAutoLinkNode, AutoLinkNode } from '@lexical/link';
import type { TextMatchTransformer } from '@lexical/markdown';
import { $createTextNode } from 'lexical';
import { findNextAutoLinkMatch } from '../utils/links';
import type { ConversionOnlyTransformer } from './transformers';

/**
 * A URL with a scheme, and whatever follows it that a URL may contain: angle
 * brackets end it, so a URL cannot reach into the node tag beside it. The
 * precise end - which trailing punctuation belongs to the sentence rather than
 * the URL - comes from the shared matcher, so this only has to be cheap and to
 * start where the URL does.
 */
const BARE_URL = /(?:https?:\/\/|mailto:)[^\s<>]+/i;

/**
 * Turn a bare URL in the markdown into a link.
 *
 * Text typed into an editor is autolinked as it is written, so markdown the app
 * composed carries its links as `m-link`. Markdown written anywhere else - an
 * agent's prose, an import - has the URL as plain text, and a static render has
 * no typing to autolink. Doing it in the conversion means a URL reads as a link
 * wherever the markdown is parsed, including in messages already sent.
 *
 * Conversion-only: while typing, an editor autolinks through its own transform,
 * with the match mode that editor was configured for.
 */
export const I_AUTOLINK: ConversionOnlyTransformer<TextMatchTransformer> = {
  dependencies: [AutoLinkNode],
  // Every link node exports through LINK_XML.
  export: () => null,
  importRegExp: BARE_URL,
  regExp: BARE_URL,
  getEndIndex: (_node, match) => {
    const link = findNextAutoLinkMatch(match[0]);
    if (!link || link.index !== 0) return false;
    return (match.index ?? 0) + link.raw.length;
  },
  replace: (textNode) => {
    const link = findNextAutoLinkMatch(textNode.getTextContent());
    // A scheme the app refuses to open leaves the text as text.
    if (!link) return;
    const linkNode = $createAutoLinkNode(link.url);
    const linkText = $createTextNode(link.text);
    linkText.setFormat(textNode.getFormat());
    linkNode.append(linkText);
    textNode.replace(linkNode);
  },
  type: 'text-match',
  conversionOnly: true,
};
