import { type Accessor, createContext, useContext } from 'solid-js';
import type {
  IncomingPhoneCall,
  PhoneCallEvent,
  PhoneLeg,
} from '../core/phone-call';
import type { PhonePlan } from '../core/phone-plan';

/** What the client needs to join a phone call's room. Opaque to the feature. */
export type PhoneRoomCredentials = {
  callId: string;
  roomName: string;
  serverUrl: string;
  token: string;
  participantId: string;
};

/** A phone call the viewer can join now: its room and its phone leg. */
export type PhoneCallJoin = {
  credentials: PhoneRoomCredentials;
  leg: PhoneLeg;
};

/**
 * Server operations on phone calls. Failures reject with a `PhoneCallError`
 * whose message is written for the person dialing.
 */
export type PhoneCallOperations = {
  /** Place a call to a number as typed (extensions included). */
  dial(to: string): Promise<PhoneCallJoin>;
  /** Answer a call ringing for the viewer. */
  answer(callId: string): Promise<PhoneCallJoin>;
  /** End a call for everyone, or decline it while it rings. */
  hangUp(callId: string): Promise<void>;
  /** Calls ringing for the viewer now, e.g. after a reload. */
  listIncoming(): Promise<IncomingPhoneCall[]>;
};

/** The viewer's audio connection to a phone call's room. */
export type PhoneMedia = {
  /**
   * Join the room with the microphone on, leaving any other call. Calls
   * `onDisconnected` once when this connection ends for any reason: the
   * other party hung up, the call was replaced, or the network dropped.
   */
  connect(
    credentials: PhoneRoomCredentials,
    onDisconnected: () => void
  ): Promise<void>;
  /** Leave the room on this device. */
  disconnect(): Promise<void>;
  /** Whether the viewer is on any call now, phone or not. */
  inAnyCall: Accessor<boolean>;
  isMuted: Accessor<boolean>;
  toggleMute(): Promise<void>;
  /** Send a keypad tone to the other party. */
  sendDigit(key: string): Promise<void>;
};

/** The viewer's phone calling setup. */
export type PhoneSettings = {
  dialingEnabled: boolean;
  /** The number outbound calls show, in E.164, when known. */
  callerId: string | null;
  /** Numbers that ring the viewer, in E.164. */
  phoneNumbers: string[];
};

export type PhoneSettingsSource = {
  /** `undefined` until loaded. */
  settings: Accessor<PhoneSettings | undefined>;
  isError: Accessor<boolean>;
};

/** The viewer's phone plan and the Phone add-on they can manage. */
export type PhonePlanSource = {
  /** `undefined` until loaded. */
  plan: Accessor<PhonePlan | undefined>;
  isError: Accessor<boolean>;
  /** The seat whose add-on is being changed, if any. */
  pendingSeat: Accessor<string | null>;
  /** Turn the Phone add-on on or off for a seat; rejects with a message. */
  setAddon(userId: string, enabled: boolean): Promise<void>;
};

/** Audible and system-level alerts for a ringing call. */
export type PhoneAlerts = {
  /** Play the ringtone until `shouldStop` or `durationMs`; returns `stop`. */
  ring(key: string, shouldStop: () => boolean, durationMs: number): () => void;
  /**
   * Show a system notification for a ringing call; resolves to a function
   * that closes it, or `undefined` when notifications are unavailable.
   */
  notify(
    call: IncomingPhoneCall,
    handlers: { answer: () => void }
  ): Promise<(() => void) | undefined>;
};

export type PhoneContext = {
  operations: PhoneCallOperations;
  media: PhoneMedia;
  settings: PhoneSettingsSource;
  alerts: PhoneAlerts;
  /**
   * Deliver the server's phone call events to `handler` for as long as the
   * calling owner lives.
   */
  subscribe(handler: (event: PhoneCallEvent) => void): void;
  /** Open the CRM contact a call was matched to. */
  openContact(contactId: string): void;
  /** Open Phone settings, where the viewer's phone plan is managed. */
  openPhoneSettings(): void;
};

const Context = createContext<PhoneContext>();
export const PhoneProvider = Context.Provider;

export function usePhoneContext(): PhoneContext {
  const context = useContext(Context);
  if (!context) throw new Error('Phone views require a PhoneProvider');
  return context;
}
