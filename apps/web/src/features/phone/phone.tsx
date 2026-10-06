import { useCallContext } from '@channel/Call/CallContext';
import { startCallRinger } from '@channel/Call/CallStartedNotifier';
import { LK_ROOM_EVENT } from '@channel/Call/livekit-loader';
import { useUserId } from '@core/context/user';
import { usePlatformNotificationState } from '@notifications';
import { createConnectionWebsocketEffect } from '@service-connection/websocket';
import { useNavigate } from '@solidjs/router';
import { type ParentProps, Show } from 'solid-js';
import { type PhoneContext, PhoneProvider } from './context/phone-context';
import { dtmfCode, remotePartyLabel } from './core/phone-call';
import { closePhoneDialer, phoneDialerRequest } from './phone-actions';
import {
  createPhoneCallOperations,
  usePhoneSettingsSource,
} from './queries/phone-calls';
import { parsePhoneCallEvent } from './queries/phone-events';
import { usePhoneCallsFlag } from './use-phone-calls-flag';
import { PhoneCallsView } from './views/phone-calls';

/**
 * App-wide phone calling: rings for calls to the viewer's numbers, shows the
 * call they are on, and opens the dialer. Mounted under the router.
 */
export function PhoneCallsProvider(props: ParentProps) {
  const flag = usePhoneCallsFlag();
  return (
    <>
      {props.children}
      <Show when={!flag().loading && flag().enabled}>
        <PhoneCalls />
      </Show>
    </>
  );
}

function PhoneCalls() {
  return (
    <PhoneProvider value={createAppPhoneContext()}>
      <PhoneCallsView
        dialerRequest={phoneDialerRequest}
        onDialerClose={closePhoneDialer}
      />
    </PhoneProvider>
  );
}

/** Only this entry point constructs production phone capabilities. */
function createAppPhoneContext(): PhoneContext {
  const call = useCallContext();
  const userId = useUserId();
  const navigate = useNavigate();
  const notification = usePlatformNotificationState();
  return {
    operations: createPhoneCallOperations(),
    settings: usePhoneSettingsSource(userId),
    media: {
      async connect(credentials, onDisconnected) {
        await call.meetingSession.connectWithToken(
          { ...credentials, channelId: null, shareToken: null },
          {
            microphoneEnabled: true,
            cameraEnabled: false,
            // Phone calls have no channel, so iOS keeps them in the browser
            // session like other standalone calls.
            useBrowserSession: true,
          }
        );
        const room = call.room();
        if (!room || call.activeCallId() !== credentials.callId) {
          onDisconnected();
          return;
        }
        room.once(LK_ROOM_EVENT.Disconnected, () => onDisconnected());
      },
      disconnect: () => call.meetingSession.disconnect(),
      inAnyCall: call.isInCall,
      isMuted: call.isAudioMuted,
      toggleMute: call.toggleAudio,
      async sendDigit(key) {
        const code = dtmfCode(key);
        const room = call.room();
        if (code === null || !room) return;
        await room.localParticipant.publishDtmf(code, key);
      },
    },
    alerts: {
      ring: (key, shouldStop, durationMs) =>
        startCallRinger(key, shouldStop, durationMs).stop,
      async notify(incoming, handlers) {
        if (notification === 'not-supported') return undefined;
        const caller = remotePartyLabel({
          contact: incoming.contact,
          remoteNumber: incoming.from,
        });
        const handle = await notification.showNotification({
          title: 'Incoming phone call',
          options: {
            body: `${caller} is calling`,
            requireInteraction: true,
            tag: `phone-call-${incoming.callId}`,
          },
        });
        if (handle === 'not-granted' || handle === 'disabled-in-ui')
          return undefined;
        handle.onClick(() => {
          window.focus();
          handlers.answer();
        });
        return () => handle.close();
      },
    },
    subscribe(handler) {
      createConnectionWebsocketEffect((frame) => {
        const event = parsePhoneCallEvent(frame.type, frame.data);
        if (event) handler(event);
      });
    },
    openContact: (contactId) => navigate(`/contact/${contactId}`),
  };
}
