/**
 * Why a phone call operation failed, in the feature's own terms:
 * - `invalid`: refused as asked (an unreadable or disallowed number);
 * - `unavailable`: phone calling isn't set up for the workspace;
 * - `gone`: the call ended or was answered elsewhere;
 * - `failed`: anything else, usually worth retrying.
 */
export type PhoneCallErrorKind = 'invalid' | 'unavailable' | 'gone' | 'failed';

/** A failed phone call operation with a message for the person dialing. */
export class PhoneCallError extends Error {
  constructor(
    readonly kind: PhoneCallErrorKind,
    message: string
  ) {
    super(message);
    this.name = 'PhoneCallError';
  }
}

/** The message to show for any error thrown by a phone operation. */
export function phoneCallErrorMessage(error: unknown): string {
  if (error instanceof PhoneCallError) return error.message;
  return 'Something went wrong with the call. Please try again.';
}
