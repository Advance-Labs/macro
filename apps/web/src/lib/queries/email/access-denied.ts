import { thrownResultErrorHasCode } from '@core/util/result';

/** Error codes that definitively revoke a viewer's cached thread source. */
export const EMAIL_ACCESS_DENIED_CODES: readonly string[] = [
  'FORBIDDEN',
  'UNAUTHORIZED',
  'NOT_FOUND',
];

export function isEmailAccessDenied(error: unknown): boolean {
  return EMAIL_ACCESS_DENIED_CODES.some((code) =>
    thrownResultErrorHasCode(error, code)
  );
}
