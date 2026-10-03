/**
 * @file Wrap the Lexical Editor in some helpful utilities.
 */
import type { Boundary } from '@floating-ui/dom';
import {
  type EditorType,
  type NodeIdMappings,
  NodeReplacements,
  nodeIdPlugin,
  RegisteredNodesByType,
  SupportedNodeTypes,
} from '@macro-inc/lexical-core';
import type { NodeKey } from 'lexical';
import {
  createEditor,
  type EditorThemeClasses,
  type LexicalEditor,
} from 'lexical';
import {
  type Accessor,
  createContext,
  createSignal,
  getOwner,
  type Owner,
} from 'solid-js';
import type { Store } from 'solid-js/store';
import {
  createPluginManager,
  insertTextPlugin,
  nodeTransformPlugin,
  type PluginManager,
  type SelectionData,
} from '../plugins';
import { theme as baseTheme } from '../theme';

type LexicalWrapperProps = {
  type: EditorType;
  namespace: string;
  isInteractable: () => boolean;
  withIds?: boolean;
  theme?: EditorThemeClasses;
  /** Overlay sources supplied by the editor host. */
  portalMount?: Accessor<HTMLElement | undefined>;
  floatingBoundary?: Accessor<Boundary | undefined>;
};

export type LexicalWrapperBase = {
  type: EditorType;
  /** Owner of the editor lifecycle, stable across decorator replacement. */
  owner: Owner | null;
  plugins: PluginManager;
  editor: LexicalEditor;
  cleanup: () => void;
  isInteractable: () => boolean;
  selection?: Store<SelectionData>;
  /** When true, decorator components should skip backend fetches (e.g. preview API). */
  skipPreviewFetch?: boolean;
  /** Stable getters follow the latest supplied accessor. */
  readonly portalMount: Accessor<HTMLElement | undefined>;
  setPortalMount: (
    source: Accessor<HTMLElement | undefined> | undefined
  ) => void;
  readonly floatingBoundary: Accessor<Boundary | undefined>;
  setFloatingBoundary: (
    source: Accessor<Boundary | undefined> | undefined
  ) => void;
};

export type LexicalWrapperWithMapping = LexicalWrapperBase & {
  mapping: NodeIdMappings;
};

export type LexicalWrapper = LexicalWrapperBase | LexicalWrapperWithMapping;

export const LexicalWrapperContext = createContext<LexicalWrapper>();

// Simple increasing id to differentiate multiple editors on page.
let _id = 0;

/**
 * Create a Lexical wrapper with extra utilities for adding plugins and tracking
 * cleanup functions.
 * @param type The type of editor to create. The current options are 'markdown',
 *     'plain-text', 'chat', and 'markdown-sync' which are each configured to include
 *     and exclude node lists tailored to those use cases.
 * @param namespace A namespace hint for the editor - useful for debugging.
 * @param isInteractable A function that returns true if when the editor should be interactable.
 * @param withIds If true, the editor will have node ids and the wrapper will have a
 *     bi-directional durable nodeId <- -> ephemeral nodeKey mapping managed by the nodeId plugin.
 */

export function createLexicalWrapper(
  props: LexicalWrapperProps & { withIds: true }
): LexicalWrapperWithMapping;

export function createLexicalWrapper(
  props: LexicalWrapperProps & { withIds?: false | undefined }
): LexicalWrapperBase;

export function createLexicalWrapper({
  type,
  namespace,
  isInteractable,
  withIds,
  theme,
  portalMount: initialPortalMount,
  floatingBoundary: initialFloatingBoundary,
}: LexicalWrapperProps): LexicalWrapper {
  const owner = getOwner();
  const [portalMountSource, setPortalMountSource] =
    createSignal(initialPortalMount);
  const [floatingBoundarySource, setFloatingBoundarySource] = createSignal(
    initialFloatingBoundary
  );
  _id++;

  const nodes = RegisteredNodesByType[type];
  const replacements = NodeReplacements.filter((replacement) => {
    if (!nodes.includes(replacement.replace)) return false;
    if (replacement.withKlass && !nodes.includes(replacement.withKlass))
      return false;

    return true;
  });

  const editor = createEditor({
    theme: theme ?? baseTheme,
    namespace: namespace + '_' + _id,
    nodes: [...nodes, ...replacements],
    onError: console.error,
  });

  const plugins = createPluginManager(editor, type);

  let mapping: NodeIdMappings | undefined;

  // Default plugins here.
  plugins.use(insertTextPlugin());
  plugins.use(nodeTransformPlugin());

  if (withIds) {
    mapping = createMapping();
    plugins.use(
      nodeIdPlugin({
        nodes: SupportedNodeTypes,
        idLength: 8,
        mappings: mapping,
      })
    );
  }

  const cleanup = () => {
    plugins.cleanup();
  };

  return {
    plugins,
    editor,
    cleanup,
    owner,
    type,
    isInteractable,
    mapping,
    portalMount: () => portalMountSource()?.(),
    setPortalMount: (source) => setPortalMountSource(() => source),
    floatingBoundary: () => floatingBoundarySource()?.(),
    setFloatingBoundary: (source) => setFloatingBoundarySource(() => source),
  };
}

export function isWrapperWithIds(
  wrapper: LexicalWrapper | undefined
): wrapper is LexicalWrapperWithMapping {
  return Boolean(
    wrapper && 'mapping' in wrapper && wrapper['mapping'] !== undefined
  );
}

function createMapping(): NodeIdMappings {
  const idToNodeKeyMap: Map<string, NodeKey> = new Map();
  const nodeKeyToIdMap: Map<NodeKey, string> = new Map();

  const nodeIdMappings: NodeIdMappings = {
    idToNodeKeyMap,
    nodeKeyToIdMap,
  };

  return nodeIdMappings;
}
