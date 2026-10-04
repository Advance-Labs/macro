import { createComponent, createRoot } from 'solid-js';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { HOTKEY_SCOPE_NEUTRAL_DATA_ATTRIBUTE } from '../dom-selectors';
import { HotkeyScope, useHotkeyScopeOrCreate } from './HotkeyScope';
import { attachGlobalDOMScope, useHotkeyDOMScope } from './hotkeys';
import { activeScope, hotkeyScopeTree, setActiveScope } from './state';

vi.mock('@app/signal/splitLayout', () => ({
  globalSplitManager: () => undefined,
}));

let container: HTMLDivElement;
let scopeEl: HTMLDivElement;
let neutralButton: HTMLButtonElement;
let plainButton: HTMLButtonElement;
let scopeId: string;
let disposeRoot: () => void;

const focusIn = (el: Element) => {
  el.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
};

beforeEach(() => {
  container = document.createElement('div');
  document.body.appendChild(container);

  scopeEl = document.createElement('div');

  const neutralZone = document.createElement('div');
  neutralZone.setAttribute(HOTKEY_SCOPE_NEUTRAL_DATA_ATTRIBUTE, '');
  neutralButton = document.createElement('button');
  neutralZone.appendChild(neutralButton);

  plainButton = document.createElement('button');

  container.append(scopeEl, neutralZone, plainButton);

  disposeRoot = createRoot((dispose) => {
    attachGlobalDOMScope(container);
    const [attachHotkeys, id] = useHotkeyDOMScope('scope-activation-test');
    attachHotkeys(scopeEl);
    scopeId = id;
    return dispose;
  });
});

afterEach(() => {
  disposeRoot();
  container.remove();
  setActiveScope('global');
});

describe('scope activation on focusin', () => {
  test('focus inside a DOM scope activates it', () => {
    focusIn(scopeEl);
    expect(activeScope()).toBe(scopeId);
  });

  test('focus in a neutral region keeps the current scope active', () => {
    focusIn(scopeEl);
    expect(activeScope()).toBe(scopeId);

    focusIn(neutralButton);
    expect(activeScope()).toBe(scopeId);

    // The neutral focusin must not leave the inner-scope-claimed flag set:
    // the next unscoped focus still falls through to global.
    focusIn(plainButton);
    expect(activeScope()).toBe('global');
  });

  test('focus outside scopes and neutral regions falls through to global', () => {
    focusIn(scopeEl);
    expect(activeScope()).toBe(scopeId);

    focusIn(plainButton);
    expect(activeScope()).toBe('global');
  });

  // The sidebar account-menu flow: opening the menu moves focus into a body
  // portal (unscoped → 'global'), closing it returns focus to the trigger in
  // the neutral sidebar. Preserving the decayed 'global' scope would leave
  // split hotkeys dead until the user clicks back into a split.
  test('returning to a neutral region after the scope decayed to global restores the last live scope', () => {
    focusIn(scopeEl);
    focusIn(plainButton);
    expect(activeScope()).toBe('global');

    focusIn(neutralButton);
    expect(activeScope()).toBe(scopeId);
  });

  // The sidebar create-menu flow: the launcher owns its own scope, which is
  // removed when it closes — so the most recent scope is dead and the one
  // before it is the scope to restore.
  test('restores the previous live scope when the last active scope was removed', () => {
    focusIn(scopeEl);

    let scopeBId = '';
    const scopeBEl = document.createElement('div');
    container.appendChild(scopeBEl);
    const disposeB = createRoot((dispose) => {
      const [attachHotkeys, id] = useHotkeyDOMScope('scope-activation-test-b');
      attachHotkeys(scopeBEl);
      scopeBId = id;
      return dispose;
    });

    focusIn(scopeBEl);
    expect(activeScope()).toBe(scopeBId);

    disposeB();
    scopeBEl.remove();
    expect(activeScope()).toBe('global');

    focusIn(neutralButton);
    expect(activeScope()).toBe(scopeId);
  });
});

describe('host hotkey scope ownership', () => {
  test('reads the split scope without creating or attaching another scope', () => {
    const sizeBefore = hotkeyScopeTree.size;
    let scope: ReturnType<typeof useHotkeyScopeOrCreate> | undefined;
    const disposeHost = createRoot((dispose) => {
      createComponent(HotkeyScope, {
        scope: scopeId,
        get children() {
          scope = useHotkeyScopeOrCreate('host');
          return null;
        },
      });
      return dispose;
    });
    try {
      expect(scope).toEqual([undefined, scopeId]);
      expect(hotkeyScopeTree.size).toBe(sizeBefore);
    } finally {
      disposeHost();
    }
    expect(hotkeyScopeTree.has(scopeId)).toBe(true);
  });

  test('attaches a standalone host scope and removes it with its owner', () => {
    const localEl = document.createElement('div');
    container.appendChild(localEl);
    let localScope = '';
    const disposeHost = createRoot((dispose) => {
      const [attach, id] = useHotkeyScopeOrCreate('standalone');
      localScope = id;
      attach?.(localEl);
      return dispose;
    });
    try {
      expect(localEl.getAttribute('data-hotkey-scope')).toBe(localScope);
      focusIn(localEl);
      expect(activeScope()).toBe(localScope);
    } finally {
      disposeHost();
    }
    expect(hotkeyScopeTree.has(localScope)).toBe(false);
  });

  test('keeps standalone host scopes separate', () => {
    const hosts = [
      document.createElement('div'),
      document.createElement('div'),
    ];
    container.append(...hosts);
    const scopes: string[] = [];
    const disposers = hosts.map((element) =>
      createRoot((dispose) => {
        const [attach, id] = useHotkeyScopeOrCreate('standalone');
        scopes.push(id);
        attach?.(element);
        return dispose;
      })
    );
    try {
      expect(scopes[0]).not.toBe(scopes[1]);
      focusIn(hosts[0]);
      expect(activeScope()).toBe(scopes[0]);
      focusIn(hosts[1]);
      expect(activeScope()).toBe(scopes[1]);
      disposers[0]();
      expect(hotkeyScopeTree.has(scopes[1])).toBe(true);
    } finally {
      disposers.forEach((dispose) => dispose());
    }
  });
});
