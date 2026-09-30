import { mergeRegister } from '@lexical/utils';
import {
  $createNodeSelection,
  $getSelection,
  $isDecoratorNode,
  $isNodeSelection,
  $isRangeSelection,
  $setSelection,
  COMMAND_PRIORITY_LOW,
  type DecoratorNode,
  KEY_ARROW_DOWN_COMMAND,
  KEY_ARROW_UP_COMMAND,
  type LexicalEditor,
  type LexicalNode,
} from 'lexical';
import { $getCaretRect } from '../../utils';

/**
 * Block decorators that arrow keys stop on: the first press selects the block,
 * the next moves past it. Lexical's own handling only stops on decorators
 * beside the caret inside one element, so top-level blocks were skipped.
 */
const STOPPING_BLOCK_TYPES = new Set([
  'image',
  'video',
  'horizontalrule',
  'database-query',
]);

type Direction = 'up' | 'down';

function $isStoppingBlock(
  node: LexicalNode | null | undefined
): node is DecoratorNode<unknown> {
  return (
    $isDecoratorNode(node) &&
    !node.isInline() &&
    node.isKeyboardSelectable() &&
    STOPPING_BLOCK_TYPES.has(node.getType())
  );
}

function $selectBlock(node: LexicalNode) {
  const selection = $createNodeSelection();
  selection.add(node.getKey());
  $setSelection(selection);
}

/** No wrapped line of `block` lies beyond the caret in `direction`. */
function $isCaretOnEdgeLine(
  editor: LexicalEditor,
  block: LexicalNode,
  direction: Direction
) {
  const element = editor.getElementByKey(block.getKey());
  const caret = $getCaretRect();
  if (!element || !caret) return false;
  const bounds = element.getBoundingClientRect();
  const beyond =
    direction === 'down'
      ? bounds.bottom - caret.bottom
      : caret.top - bounds.top;
  return beyond < Math.max(caret.height, 1);
}

function $neighbor(node: LexicalNode, direction: Direction) {
  return direction === 'down'
    ? node.getNextSibling()
    : node.getPreviousSibling();
}

function $stopOnBlock(
  editor: LexicalEditor,
  event: KeyboardEvent | null,
  direction: Direction
) {
  if (event?.shiftKey) return false;
  const selection = $getSelection();
  if ($isNodeSelection(selection)) {
    const [node] = selection.getNodes();
    if (selection.getNodes().length !== 1 || !$isStoppingBlock(node))
      return false;
    const next = $neighbor(node, direction);
    if (!$isStoppingBlock(next)) return false;
    event?.preventDefault();
    $selectBlock(next);
    return true;
  }
  if (!$isRangeSelection(selection) || !selection.isCollapsed()) return false;
  const block = selection.focus.getNode().getTopLevelElement();
  if (!block) return false;
  const next = $neighbor(block, direction);
  if (!$isStoppingBlock(next)) return false;
  if (!$isCaretOnEdgeLine(editor, block, direction)) return false;
  event?.preventDefault();
  $selectBlock(next);
  return true;
}

export function blockDecoratorNavigationPlugin() {
  return (editor: LexicalEditor) =>
    mergeRegister(
      editor.registerCommand(
        KEY_ARROW_DOWN_COMMAND,
        (event) => $stopOnBlock(editor, event, 'down'),
        COMMAND_PRIORITY_LOW
      ),
      editor.registerCommand(
        KEY_ARROW_UP_COMMAND,
        (event) => $stopOnBlock(editor, event, 'up'),
        COMMAND_PRIORITY_LOW
      )
    );
}
