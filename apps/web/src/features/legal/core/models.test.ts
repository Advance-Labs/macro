import { describe, expect, it } from 'vitest';
import {
  type Draft,
  isExpired,
  type SigningSession,
  sendIssue,
  signingIssue,
} from './models';

const draft = (): Draft => ({
  title: 'NDA',
  message: '',
  revision: 1,
  recipients: [
    {
      id: 'signer',
      name: 'Alex',
      email: 'alex@example.com',
      order: 1,
      signedAt: null,
      deliveredAt: null,
    },
  ],
  fields: [
    {
      id: 'signature',
      recipientId: 'signer',
      kind: 'signature',
      page: 1,
      x: 0.1,
      y: 0.1,
      width: 0.2,
      height: 0.05,
      required: true,
      value: null,
    },
  ],
});
describe('envelope preparation feedback', () => {
  it('requires a unique valid email and a signature for every recipient', () => {
    expect(sendIssue(draft())).toBeUndefined();
    const missing = draft();
    missing.fields = [];
    expect(sendIssue(missing)).toBe('Place a required signature for Alex.');
    const duplicate = draft();
    duplicate.recipients.push({ ...duplicate.recipients[0], id: 'second' });
    expect(sendIssue(duplicate)).toBe('Each recipient needs a unique email.');
    const invalid = draft();
    invalid.recipients[0].email = 'invalid';
    expect(sendIssue(invalid)).toBe(
      'Add a name and valid email for every recipient.'
    );
  });
  it('treats expiry as a pending-envelope state', () => {
    expect(
      isExpired(
        { status: 'sent', expiresAt: '2026-01-01T00:00:00Z' },
        Date.parse('2026-01-02')
      )
    ).toBe(true);
    expect(
      isExpired(
        { status: 'completed', expiresAt: '2026-01-01T00:00:00Z' },
        Date.parse('2026-01-02')
      )
    ).toBe(false);
  });
  it('requires intent and entered values while dates are server-owned', () => {
    const session = {
      fields: [
        ...draft().fields,
        { ...draft().fields[0], id: 'date', kind: 'date' },
      ],
    } as SigningSession;
    expect(signingIssue(session, { signature: 'Alex' }, false)).toMatch(
      /Agree/
    );
    expect(signingIssue(session, {}, true)).toMatch(/required/);
    expect(signingIssue(session, { signature: 'Alex' }, true)).toBeUndefined();
  });
});
