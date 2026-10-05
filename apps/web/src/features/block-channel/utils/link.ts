import { channelLocationUpdates } from '@app/features/channels-view/channels-route';
import { globalSplitManager } from '@app/signal/splitLayout';
import { URL_PARAMS } from '@block-channel/constants';
import type {
  OpenSplitResult,
  SplitHandle,
  SplitManager,
} from '@components/app/split-layout/layoutManager';

export function getChannelParams(
  messageId: string,
  threadId?: string
): Record<string, string> {
  const params: Record<string, string> = {};
  params[URL_PARAMS.message] = messageId;

  if (threadId) {
    params[URL_PARAMS.thread] = threadId;
  }

  return params;
}

export async function navigateToChannelMessage(
  channelId: string,
  messageId: string,
  threadId?: string,
  options?: {
    splitManager?: SplitManager;
    preferNewSplit?: boolean;
    /** The split this navigation originates from. */
    sourceHandle?: SplitHandle;
    /** Runs once the destination has actually been applied or reused. */
    onApplied?: VoidFunction;
  }
) {
  const splitManager = options?.splitManager ?? globalSplitManager();
  if (!splitManager) return;

  let onApplied = options?.onApplied;
  const reportApplied = () => {
    const callback = onApplied;
    onApplied = undefined;
    callback?.();
  };
  const reportImmediateResult = (result: OpenSplitResult | undefined) => {
    if (!result) return;
    if (result.status === 'opened' || result.status === 'reused') {
      reportApplied();
    }
  };

  const result = splitManager.openWithSplit(
    { type: 'channel', id: channelId },
    {
      activate: true,
      referredFrom: null,
      preferNewSplit: options?.preferNewSplit,
      handle: options?.sourceHandle,
      search: channelLocationUpdates({ kind: 'message', messageId, threadId }),
      ...(options?.onApplied ? { onApplied: reportApplied } : {}),
    }
  );
  reportImmediateResult(result);
}
