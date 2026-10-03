import { createContext, useContext } from 'solid-js';
import type { Workspace } from '../primitives/workspace';

export const LegalWorkspaceContext = createContext<Workspace>();
export function useLegalWorkspace() {
  const workspace = useContext(LegalWorkspaceContext);
  if (!workspace) throw new Error('Legal views require a Legal workspace.');
  return workspace;
}
