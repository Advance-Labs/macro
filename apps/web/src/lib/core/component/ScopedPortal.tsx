import { getSplitPanelRef } from '@components/app/split-layout/layoutUtils';
import type { ComponentProps } from 'solid-js';
import { Portal, Show } from 'solid-js/web';

export type PortalScope = 'local' | 'global' | 'split';

/** Resolve a named portal scope from the owning host's DOM capabilities. */
export function resolveScopedPortalMount(
  scope: PortalScope | undefined,
  searchRef?: HTMLElement,
  splitRef?: HTMLElement | null
): HTMLElement {
  if (scope === 'split') {
    return (
      splitRef ??
      searchRef?.closest<HTMLElement>('[data-split-panel]') ??
      document.body
    );
  }
  if (scope === 'local') {
    return searchRef?.closest<HTMLElement>('.portal-scope') ?? document.body;
  }
  return document.body;
}
/**
 * Portal with some extra scoping logic. If passed a specific mount prop or no props at all – it is
 * just a regular solid Portal.
 * @param props.scope - The scope of the portal. If 'local' it will mount to the closest element with the
 *    '.portal-scope' class. If 'split' it will mount to the containing split panel. If 'global' it will
 *    mount to the document body.
 * @returns
 */
export function ScopedPortal(
  props: ComponentProps<typeof Portal> & {
    scope?: PortalScope;
    show?: boolean;
  }
) {
  let searchRef!: HTMLDivElement;

  const mountRef = () =>
    props.mount ??
    resolveScopedPortalMount(
      props.scope,
      searchRef,
      props.scope === 'split' ? getSplitPanelRef() : undefined
    );

  return (
    <Show when={props.show !== false}>
      <div class="hidden" ref={searchRef} />
      <Portal mount={mountRef()}>{props.children}</Portal>
    </Show>
  );
}
