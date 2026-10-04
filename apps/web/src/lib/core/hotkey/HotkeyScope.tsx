import { createContext, type FlowProps, useContext } from 'solid-js';
import { useHotkeyDOMScope } from './hotkeys';

const HotkeyScopeContext = createContext<string>();

/**
 * Exposes an existing host scope without creating or attaching another scope.
 * Scope identity and inheritance stay fixed for the provider's mounted lifetime.
 * Updating `scope` does not retarget commands. Use explicit reactive scope inputs
 * for commands that need to move between scopes without remounting their host.
 */
export function HotkeyScope(props: FlowProps<{ scope?: string }>) {
  const parentScope = useMaybeHotkeyScope();
  const scope = props.scope ?? parentScope;
  if (!scope) {
    throw new Error('HotkeyScope requires a scope or a parent HotkeyScope.');
  }
  return (
    <HotkeyScopeContext.Provider value={scope}>
      {props.children}
    </HotkeyScopeContext.Provider>
  );
}

/** Reads the nearest host scope without falling back to global shortcuts. */
export function useMaybeHotkeyScope() {
  return useContext(HotkeyScopeContext);
}

export function useHotkeyScope() {
  const scope = useMaybeHotkeyScope();
  if (!scope) {
    throw new Error('useHotkeyScope requires a HotkeyScope provider.');
  }
  return scope;
}

/**
 * Reuses the host scope, or creates a standalone scope for the caller to attach.
 * The scope and ownership choice stay fixed for the caller's mounted lifetime.
 */
export function useHotkeyScopeOrCreate(
  prefix: string
): [((element: Element) => void) | undefined, string] {
  const scope = useMaybeHotkeyScope();
  if (scope) return [undefined, scope];
  return useHotkeyDOMScope(prefix);
}
