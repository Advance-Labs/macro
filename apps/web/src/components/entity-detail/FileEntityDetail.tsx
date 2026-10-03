import {
  DocumentFileSidePanelSections,
  SidePanel,
} from '@components/app/side-panel';
import {
  getPermissions,
  hasPermissions,
  Permissions,
} from '@core/component/SharePermissions';
import type { AccessLevel } from '@service-storage/generated/schemas/accessLevel';
import type { DocumentMetadata } from '@service-storage/generated/schemas/documentMetadata';
import type { JSX, ParentProps } from 'solid-js';
import type { FileDetailContext } from './file-detail-context';
import { downloadFileOperation } from './file-detail-operations';

export type FileDetailLayoutProps = ParentProps<{
  documentId: string;
  documentMetadata: DocumentMetadata;
  userAccessLevel: AccessLevel;
  defaultSidePanelOpen?: boolean;
}>;

export function FileDetailLayout(props: FileDetailLayoutProps) {
  const permissions = () => getPermissions(props.userAccessLevel);
  const canEdit = () => hasPermissions(permissions(), Permissions.CAN_EDIT);

  return (
    <SidePanel.Layout
      floating
      defaultOpen={props.defaultSidePanelOpen ?? false}
      persistKey={`file:${props.documentId}`}
      headerToggle={false}
    >
      <DocumentFileSidePanelSections
        documentId={props.documentId}
        documentName={props.documentMetadata.documentName}
        canEdit={canEdit()}
      />
      <div class="relative size-full min-h-0 min-w-0 overflow-hidden">
        {props.children}
      </div>
    </SidePanel.Layout>
  );
}

/** Hosts compose shared file chrome around an already-loaded feature body. */
export function FileEntityDetail<T>(
  props: Omit<FileDetailLayoutProps, 'children'> & {
    data: T;
    content: JSX.Element;
    onDownload?: () => void;
    children?: (context: FileDetailContext<T>) => JSX.Element;
  }
) {
  const context: FileDetailContext<T> = {
    get data() {
      return props.data;
    },
    get documentMetadata() {
      return props.documentMetadata;
    },
    get userAccessLevel() {
      return props.userAccessLevel;
    },
    get operations() {
      return props.onDownload
        ? [downloadFileOperation(() => props.onDownload?.())]
        : undefined;
    },
  };

  return (
    <FileDetailLayout
      documentId={props.documentId}
      documentMetadata={props.documentMetadata}
      userAccessLevel={props.userAccessLevel}
      defaultSidePanelOpen={props.defaultSidePanelOpen}
    >
      {props.children?.(context)}
      {props.content}
    </FileDetailLayout>
  );
}
