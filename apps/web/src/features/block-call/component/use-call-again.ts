import { useQuickCallsFlag } from '@app/features/meetings/use-quick-calls-flag';
import {
  callPhoneNumber,
  usePhoneDialingAvailable,
} from '@app/features/phone/phone-actions';
import { getMeetingPath } from '@channel/Call/call-link';
import { joinChannelCall } from '@channel/Call/join-channel-call';
import { useCallLinkQuery } from '@queries/call/meetings';
import { useNavigate } from '@solidjs/router';
import type { Accessor } from 'solid-js';

/**
 * Rejoin a channel call, open the invitation for a standalone meeting, or
 * dial the other party of a phone call again.
 */
export function useCallAgain(
  callId: Accessor<string>,
  channelId: Accessor<string | null | undefined>,
  phoneNumber: Accessor<string | undefined> = () => undefined
) {
  const navigate = useNavigate();
  const flag = useQuickCallsFlag();
  const dialing = usePhoneDialingAvailable();
  const quickCallsEnabled = () => !flag().loading && flag().enabled;
  const meeting = useCallLinkQuery(() =>
    quickCallsEnabled() && !channelId() && !phoneNumber() ? callId() : undefined
  );
  const shareToken = () =>
    quickCallsEnabled() && meeting.isSuccess
      ? meeting.data?.shareToken
      : undefined;
  const canCallAgain = () =>
    phoneNumber() ? dialing() : Boolean(channelId() || shareToken());
  const callAgain = () => {
    const number = phoneNumber();
    if (number) {
      if (dialing()) callPhoneNumber(number);
      return;
    }
    const channel = channelId();
    if (channel) {
      void joinChannelCall(channel);
      return;
    }
    const token = shareToken();
    if (token) navigate(getMeetingPath(token));
  };

  return { canCallAgain, callAgain };
}
