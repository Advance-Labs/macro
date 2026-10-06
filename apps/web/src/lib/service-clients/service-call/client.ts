import { SERVER_HOSTS } from '@core/constant/servers';
import { fetchWithToken } from '@core/util/fetchWithToken';
import { safeFetch, statusError } from '@core/util/safeFetch';

import type { ActiveCallsResponse } from '@service-storage/generated/schemas/activeCallsResponse';
import type { ActiveMeeting as ApiActiveMeeting } from '@service-storage/generated/schemas/activeMeeting';
import type { CallActiveResponse } from '@service-storage/generated/schemas/callActiveResponse';
import type { CallRecord } from '@service-storage/generated/schemas/callRecord';
import type { CallTokenResponse as ApiCallTokenResponse } from '@service-storage/generated/schemas/callTokenResponse';
import type { CreateMeetingRequest } from '@service-storage/generated/schemas/createMeetingRequest';
import type { DialPhoneRequest } from '@service-storage/generated/schemas/dialPhoneRequest';
import type { EditCallRecordRequest } from '@service-storage/generated/schemas/editCallRecordRequest';
import type { IncomingPhoneCallsResponse } from '@service-storage/generated/schemas/incomingPhoneCallsResponse';
import type { InviteMeetingUsersRequest } from '@service-storage/generated/schemas/inviteMeetingUsersRequest';
import type { LeaveCallResponse } from '@service-storage/generated/schemas/leaveCallResponse';
import type { Meeting as ApiMeeting } from '@service-storage/generated/schemas/meeting';
import type { MeetingPreparation } from '@service-storage/generated/schemas/meetingPreparation';
import type { PhoneCallJoinResponse } from '@service-storage/generated/schemas/phoneCallJoinResponse';
import type { PhoneSettingsResponse } from '@service-storage/generated/schemas/phoneSettingsResponse';
import type { UpdateMeetingRequest } from '@service-storage/generated/schemas/updateMeetingRequest';
import type { UpdateSharePermissionRequestV2 } from '@service-storage/generated/schemas/updateSharePermissionRequestV2';

export type {
  CallRecord,
  CreateMeetingRequest,
  IncomingPhoneCallsResponse,
  PhoneCallJoinResponse,
  PhoneSettingsResponse,
  UpdateMeetingRequest,
};

// Rust serializes these nullable fields explicitly; Orval marks Option<T> optional.
export type CallTokenResponse = Required<ApiCallTokenResponse>;
export type Meeting = Required<ApiMeeting>;
export type ActiveMeeting = Required<ApiActiveMeeting>;

/** Display-only waiting-room data; no account identities or call contents. */
export type MeetingParticipants = {
  participants: { displayName: string; avatarUrl: string | null }[];
};

const host: string = SERVER_HOSTS['document-storage-service'];

/**
 * `PHONE_INVALID` — the request was refused as written (an unreadable or
 * disallowed number, a call that can no longer be answered); `PHONE_UNAVAILABLE`
 * — phone calling isn't set up for the deployment; `PHONE_PAYMENT_REQUIRED` —
 * the caller's plan doesn't pay for the call (no phone plan, or minutes and
 * usage billing used up).
 */
export type PhoneErrorCode =
  | 'PHONE_INVALID'
  | 'PHONE_UNAVAILABLE'
  | 'PHONE_PAYMENT_REQUIRED';

function phoneErrorCode(status: number): PhoneErrorCode | undefined {
  switch (status) {
    case 400:
      return 'PHONE_INVALID';
    case 402:
      return 'PHONE_PAYMENT_REQUIRED';
    case 503:
      return 'PHONE_UNAVAILABLE';
    default:
      return undefined;
  }
}

/** Phone endpoints explain refusals in words meant for the person dialing. */
async function phoneErrorResponse(response: Response) {
  const failure = statusError(response.status);
  const code = phoneErrorCode(response.status);
  if (!code) return failure;
  const body: unknown = await response.json().catch(() => undefined);
  const message =
    body && typeof body === 'object' && 'message' in body
      ? body.message
      : undefined;
  return {
    code,
    message: typeof message === 'string' && message ? message : failure.message,
  };
}

export const callServiceClient = {
  prepareMeeting() {
    return fetchWithToken<MeetingPreparation>(`${host}/call/meetings/prepare`, {
      method: 'POST',
    });
  },
  cancelMeetingPreparation(id: string) {
    return fetchWithToken<Record<string, never>>(
      `${host}/call/meetings/prepare/${encodeURIComponent(id)}`,
      { method: 'DELETE', keepalive: true }
    );
  },
  inviteMeetingUsers(shareToken: string, userIds: string[]) {
    const body: InviteMeetingUsersRequest = { userIds };
    return fetchWithToken<Record<string, never>>(
      `${host}/call/meetings/invite/${encodeURIComponent(shareToken)}/users`,
      { method: 'POST', body: JSON.stringify(body) }
    );
  },
  createMeeting(body: CreateMeetingRequest) {
    return fetchWithToken<Meeting>(`${host}/call/meetings`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  },

  updateMeeting(meetingId: string, body: UpdateMeetingRequest) {
    return fetchWithToken<Meeting>(
      `${host}/call/meetings/${encodeURIComponent(meetingId)}`,
      { method: 'PATCH', body: JSON.stringify(body) }
    );
  },

  async getMeetings() {
    return (
      await fetchWithToken<{ meetings: Meeting[] }, 'MEETINGS_UNAVAILABLE'>(
        `${host}/call/meetings`,
        {
          errorResponseHandler: async (response) => {
            if (response.status === 400) {
              const body = await response.json().catch(() => undefined);
              // Older servers route the literal "meetings" as a channel UUID.
              if (body?.message === 'Bad request: Invalid channel ID format')
                return {
                  code: 'MEETINGS_UNAVAILABLE',
                  message:
                    'Quick and scheduled calls are not available on this server yet.',
                };
            }
            return {
              code:
                response.status === 401
                  ? 'UNAUTHORIZED'
                  : response.status === 403
                    ? 'FORBIDDEN'
                    : response.status === 404
                      ? 'NOT_FOUND'
                      : response.status >= 500
                        ? 'SERVER_ERROR'
                        : 'HTTP_ERROR',
              message: `HTTP error! status: ${response.status}`,
            };
          },
        }
      )
    ).map((result) => result.meetings);
  },

  /** Live Quick Calls the current user created, attended, or was invited to. */
  async getActiveMeetings() {
    return (
      await fetchWithToken<{ meetings: ActiveMeeting[] }>(
        `${host}/call/meetings/active`
      )
    ).map((result) => result.meetings);
  },

  cancelMeeting(meetingId: string) {
    return fetchWithToken<Record<string, never>>(
      `${host}/call/meetings/${encodeURIComponent(meetingId)}`,
      { method: 'DELETE' }
    );
  },

  getCallLink(callId: string) {
    return fetchWithToken<Meeting>(
      `${host}/call/record/${encodeURIComponent(callId)}/link`,
      { method: 'POST' }
    );
  },

  getMeeting(shareToken: string) {
    return safeFetch<Meeting>(
      `${host}/call/join/${encodeURIComponent(shareToken)}`,
      { credentials: 'omit' }
    );
  },

  getMeetingParticipants(shareToken: string, authenticated: boolean) {
    const path = `/join/${encodeURIComponent(shareToken)}/participants`;
    return authenticated
      ? fetchWithToken<MeetingParticipants>(`${host}/call/meetings${path}`)
      : safeFetch<MeetingParticipants>(`${host}/call${path}`, {
          credentials: 'omit',
        });
  },

  joinMeeting(shareToken: string) {
    return fetchWithToken<CallTokenResponse>(
      `${host}/call/meetings/join/${encodeURIComponent(shareToken)}`,
      { method: 'POST' }
    );
  },

  joinMeetingAsGuest(shareToken: string, displayName: string) {
    return safeFetch<CallTokenResponse>(
      `${host}/call/join/${encodeURIComponent(shareToken)}`,
      {
        method: 'POST',
        credentials: 'omit',
        body: JSON.stringify({ displayName }),
      }
    );
  },

  leaveMeeting(shareToken: string, token: string) {
    return safeFetch<LeaveCallResponse>(
      `${host}/call/join/${encodeURIComponent(shareToken)}/leave`,
      {
        method: 'POST',
        keepalive: true,
        credentials: 'omit',
        headers: { Authorization: `Bearer ${token}` },
      }
    );
  },

  async getOrCreateCall(channelId: string) {
    return (
      await fetchWithToken<CallTokenResponse>(`${host}/call/${channelId}`, {
        method: 'GET',
      })
    ).map((result) => result);
  },

  async leaveCall(channelId: string) {
    return (
      await fetchWithToken<LeaveCallResponse>(`${host}/call/${channelId}`, {
        method: 'DELETE',
      })
    ).map((result) => result);
  },

  async checkActiveCall(channelId: string) {
    return (
      await fetchWithToken<CallActiveResponse>(
        `${host}/call/${channelId}/active`,
        { method: 'GET' }
      )
    ).map(
      // safeFetch returns {} for 204 (no Content-Type header)
      (data) => ('callId' in data ? (data as CallActiveResponse) : null)
    );
  },

  async getActiveCalls() {
    return (
      await fetchWithToken<ActiveCallsResponse>(`${host}/call/active`, {
        method: 'GET',
      })
    ).map((response) => response.calls ?? []);
  },

  async getCallRecord(callId: string) {
    return (
      await fetchWithToken<CallRecord>(`${host}/call/record/${callId}`, {
        method: 'GET',
      })
    ).map((result) => result);
  },

  async deleteCallRecord(callId: string) {
    return (
      await fetchWithToken<Record<string, never>>(
        `${host}/call/record/${callId}`,
        { method: 'DELETE' }
      )
    ).map(() => undefined);
  },

  /**
   * `POST /call/record/{id}/share-with-team/toggle`: flips the live call's
   * share-with-team toggle and returns the new value. The toggle becomes
   * canonical team sharing (view for the creator's team) when the call is
   * archived; archived calls answer 409 and are edited via `editCallRecord`.
   */
  async toggleShareWithTeam(callId: string) {
    // fetchWithToken requires T extends ObjectLike, but this endpoint returns a
    // primitive JSON boolean. response.json() parses it correctly at runtime;
    // we only need to satisfy the generic constraint.
    const result = await fetchWithToken<Record<string, never>>(
      `${host}/call/record/${callId}/share-with-team/toggle`,
      { method: 'POST' }
    );
    return result.map((r) => r as unknown as boolean);
  },

  /**
   * `PATCH /call/record/{id}`. Team sharing goes through
   * `sharePermission.teamShareAccessLevel`, capped at `'view'` (`null`
   * revokes). While the call is live it sets the pending toggle; once the
   * call is archived the backend authorizes it against the call's creator.
   */
  async editCallRecord(params: {
    callId: string;
    customName?: string;
    sharePermission?: UpdateSharePermissionRequestV2;
  }) {
    const body: EditCallRecordRequest = {};
    if (params.customName !== undefined) body.customName = params.customName;
    if (params.sharePermission !== undefined)
      body.sharePermission = params.sharePermission;

    return (
      await fetchWithToken<Record<string, never>>(
        `${host}/call/record/${params.callId}`,
        {
          method: 'PATCH',
          body: JSON.stringify(body),
        }
      )
    ).map(() => undefined);
  },

  /** `GET /call/phone/settings`: whether the caller can dial out, and their numbers. */
  getPhoneSettings() {
    return fetchWithToken<PhoneSettingsResponse>(`${host}/call/phone/settings`);
  },

  /**
   * `POST /call/phone/dial`: place a phone call. `to` is the number as typed;
   * the server parses it, extensions included. Join the returned call to
   * hear it ring.
   */
  dialPhone(to: string) {
    const body: DialPhoneRequest = { to };
    return fetchWithToken<PhoneCallJoinResponse, PhoneErrorCode>(
      `${host}/call/phone/dial`,
      {
        method: 'POST',
        body: JSON.stringify(body),
        errorResponseHandler: phoneErrorResponse,
      }
    );
  },

  /** `GET /call/phone/incoming`: phone calls ringing for the caller. */
  getIncomingPhoneCalls() {
    return fetchWithToken<IncomingPhoneCallsResponse>(
      `${host}/call/phone/incoming`
    );
  },

  /** `POST /call/phone/{id}/answer`: answer a ringing phone call. */
  answerPhoneCall(callId: string) {
    return fetchWithToken<PhoneCallJoinResponse, PhoneErrorCode>(
      `${host}/call/phone/${encodeURIComponent(callId)}/answer`,
      { method: 'POST', errorResponseHandler: phoneErrorResponse }
    );
  },

  /**
   * `POST /call/phone/{id}/hang-up`: end a phone call for everyone, or
   * decline it while it rings.
   */
  hangUpPhoneCall(callId: string) {
    return fetchWithToken<LeaveCallResponse>(
      `${host}/call/phone/${encodeURIComponent(callId)}/hang-up`,
      { method: 'POST', keepalive: true }
    );
  },
};
