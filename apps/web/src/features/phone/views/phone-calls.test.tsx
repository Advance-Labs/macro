import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  type PhoneContext,
  PhoneProvider,
  type PhoneSettings,
} from '../context/phone-context';
import type { PhoneCallEvent, PhoneLeg } from '../core/phone-call';
import { PhoneCallError } from '../core/phone-call-error';
import type { DialerRequest } from './dialer-dialog';
import { PhoneCallsView } from './phone-calls';

afterEach(cleanup);

const leg = (overrides: Partial<PhoneLeg> = {}): PhoneLeg => ({
  direction: 'inbound',
  status: 'ringing',
  remoteNumber: '+15552345678',
  localNumber: '+15559870000',
  participantIdentity: 'sip_+15552345678',
  contact: { contactId: 'contact-1', name: 'Ada Lovelace' },
  answeredAt: null,
  endedAt: null,
  ...overrides,
});

const credentials = (callId: string) => ({
  callId,
  roomName: callId,
  serverUrl: 'wss://rtc.example',
  token: 'token',
  participantId: 'macro|me@example.com',
});

function setup(
  options: { settings?: PhoneSettings; dialerRequest?: DialerRequest } = {}
) {
  let emit: (event: PhoneCallEvent) => void = () => undefined;
  const [request, setRequest] = createSignal<DialerRequest | null>(
    options.dialerRequest ?? null
  );
  const context = {
    operations: {
      dial: vi.fn(async (_to: string) => ({
        credentials: credentials('call-out'),
        leg: leg({ direction: 'outbound', status: 'dialing' }),
      })),
      answer: vi.fn(async (callId: string) => ({
        credentials: credentials(callId),
        leg: leg({ status: 'active' }),
      })),
      hangUp: vi.fn(async (_callId: string) => undefined),
      listIncoming: vi.fn(async () => []),
    },
    media: {
      connect: vi.fn(async () => undefined),
      disconnect: vi.fn(async () => undefined),
      inAnyCall: () => false,
      isMuted: () => false,
      toggleMute: vi.fn(async () => undefined),
      sendDigit: vi.fn(async () => undefined),
    },
    settings: {
      settings: () =>
        options.settings ?? {
          dialingEnabled: true,
          callerId: '+15559870000',
          phoneNumbers: ['+15559870000'],
        },
      isError: () => false,
    },
    alerts: {
      ring: vi.fn(() => vi.fn()),
      notify: vi.fn(async () => undefined),
    },
    subscribe: (handler: (event: PhoneCallEvent) => void) => {
      emit = handler;
    },
    openContact: vi.fn(),
  } satisfies PhoneContext;
  render(() => (
    <PhoneProvider value={context}>
      <PhoneCallsView
        dialerRequest={request}
        onDialerClose={() => setRequest(null)}
      />
    </PhoneProvider>
  ));
  return { context, emit: (event: PhoneCallEvent) => emit(event), request };
}

const incoming: PhoneCallEvent = {
  type: 'incoming',
  call: {
    callId: 'call-in',
    from: '+15552345678',
    to: '+15559870000',
    contact: { contactId: 'contact-1', name: 'Ada Lovelace' },
    startedAt: new Date().toISOString(),
  },
};

describe('PhoneCallsView', () => {
  it('rings for an incoming call and joins it on answer', async () => {
    const { context, emit } = setup();

    emit(incoming);
    expect(
      await screen.findByRole('region', {
        name: 'Incoming phone call from Ada Lovelace',
      })
    ).toBeTruthy();
    expect(screen.getByText('To +1 (555) 987-0000')).toBeTruthy();
    expect(context.alerts.ring).toHaveBeenCalledOnce();

    fireEvent.click(
      screen.getByRole('button', { name: 'Answer call from Ada Lovelace' })
    );

    expect(
      await screen.findByRole('region', {
        name: 'Phone call with Ada Lovelace',
      })
    ).toBeTruthy();
    expect(context.operations.answer).toHaveBeenCalledWith('call-in');
    expect(context.media.connect).toHaveBeenCalledOnce();
    await vi.waitFor(() =>
      expect(
        screen.queryByRole('region', {
          name: 'Incoming phone call from Ada Lovelace',
        })
      ).toBeNull()
    );
  });

  it('stops ringing when the caller hangs up first', async () => {
    const { emit } = setup();
    emit(incoming);
    await screen.findByRole('region', {
      name: 'Incoming phone call from Ada Lovelace',
    });

    emit({
      type: 'updated',
      callId: 'call-in',
      leg: leg({ status: 'missed' }),
    });

    expect(
      screen.queryByRole('region', {
        name: 'Incoming phone call from Ada Lovelace',
      })
    ).toBeNull();
  });

  it('declines a ringing call', async () => {
    const { context, emit } = setup();
    emit(incoming);

    fireEvent.click(
      await screen.findByRole('button', {
        name: 'Decline call from Ada Lovelace',
      })
    );

    expect(context.operations.hangUp).toHaveBeenCalledWith('call-in');
  });

  it('keeps a failed answer on the card with its reason', async () => {
    const { context, emit } = setup();
    context.operations.answer.mockRejectedValueOnce(
      new PhoneCallError('failed', 'Could not reach Macro.')
    );
    emit(incoming);

    fireEvent.click(
      await screen.findByRole('button', {
        name: 'Answer call from Ada Lovelace',
      })
    );

    expect((await screen.findByRole('alert')).textContent).toBe(
      'Could not reach Macro.'
    );
  });

  it('dials from the dialer and closes it once connected', async () => {
    const { context, request } = setup({
      dialerRequest: { number: '', autoDial: false },
    });
    const input = await screen.findByRole('textbox', { name: 'Phone number' });
    expect(screen.getByText('Calling from +1 (555) 987-0000')).toBeTruthy();
    const call = screen.getByRole('button', { name: 'Call' });
    expect((call as HTMLButtonElement).disabled).toBe(true);

    fireEvent.input(input, { target: { value: '(555) 234-5678' } });
    fireEvent.click(call);

    await vi.waitFor(() => expect(request()).toBeNull());
    expect(context.operations.dial).toHaveBeenCalledWith('(555) 234-5678');
    expect(
      await screen.findByRole('region', {
        name: 'Phone call with Ada Lovelace',
      })
    ).toBeTruthy();
  });

  it('explains when the workspace cannot dial out', async () => {
    setup({
      settings: { dialingEnabled: false, callerId: null, phoneNumbers: [] },
      dialerRequest: { number: '5552345678', autoDial: false },
    });

    expect(
      await screen.findByText(
        "Phone calling isn't set up for your workspace yet."
      )
    ).toBeTruthy();
    expect(
      (screen.getByRole('button', { name: 'Call' }) as HTMLButtonElement)
        .disabled
    ).toBe(true);
  });
});
