import { z } from 'zod';

export const statusSchema = z.enum([
  'draft',
  'sent',
  'completed',
  'declined',
  'voided',
]);
export const fieldKindSchema = z.enum([
  'signature',
  'initials',
  'date',
  'text',
]);
export const recipientSchema = z.object({
  id: z.string().uuid(),
  name: z.string(),
  email: z.string(),
  order: z.number(),
  signedAt: z.string().nullable(),
  deliveredAt: z.string().nullable(),
});
export const fieldSchema = z.object({
  id: z.string().uuid(),
  recipientId: z.string().uuid(),
  kind: fieldKindSchema,
  page: z.number(),
  x: z.number(),
  y: z.number(),
  width: z.number(),
  height: z.number(),
  required: z.boolean(),
  value: z.string().nullable(),
});
export const auditSchema = z.object({
  at: z.string(),
  action: z.enum([
    'created',
    'updated',
    'sent',
    'delivered',
    'signed',
    'completed',
    'declined',
    'voided',
  ]),
  actor: z.string(),
  detail: z.string(),
});
export const envelopeSchema = z.object({
  id: z.string().uuid(),
  title: z.string(),
  message: z.string(),
  filename: z.string(),
  pageCount: z.number(),
  sourceSha256: z.string(),
  completedSha256: z.string().nullable(),
  status: statusSchema,
  revision: z.number(),
  createdAt: z.string(),
  updatedAt: z.string(),
  expiresAt: z.string().nullable(),
  recipients: z.array(recipientSchema),
  fields: z.array(fieldSchema),
  audit: z.array(auditSchema),
});
export const sessionSchema = envelopeSchema
  .pick({
    id: true,
    title: true,
    message: true,
    filename: true,
    sourceSha256: true,
    pageCount: true,
    status: true,
    revision: true,
    expiresAt: true,
  })
  .extend({
    recipient: recipientSchema,
    canSign: z.boolean(),
    fields: z.array(fieldSchema),
  });
export type Envelope = z.infer<typeof envelopeSchema>;
export type Recipient = z.infer<typeof recipientSchema>;
export type Field = z.infer<typeof fieldSchema>;
export type FieldKind = z.infer<typeof fieldKindSchema>;
export type SigningSession = z.infer<typeof sessionSchema>;
export type Status = z.infer<typeof statusSchema>;
export type Draft = Pick<
  Envelope,
  'title' | 'message' | 'revision' | 'recipients' | 'fields'
>;
export type SignatureSubmission = {
  revision: number;
  consent: boolean;
  values: { fieldId: string; value: string }[];
};
export const statusLabels: Record<Status, string> = {
  draft: 'Draft',
  sent: 'Waiting for others',
  completed: 'Completed',
  declined: 'Declined',
  voided: 'Voided',
};
export const fieldLabels: Record<FieldKind, string> = {
  signature: 'Signature',
  initials: 'Initials',
  date: 'Date signed',
  text: 'Text',
};
export function isExpired(
  envelope: Pick<Envelope, 'status' | 'expiresAt'>,
  now = Date.now()
) {
  return (
    envelope.status === 'sent' &&
    !!envelope.expiresAt &&
    Date.parse(envelope.expiresAt) <= now
  );
}
export function sendIssue(draft: Draft) {
  if (!draft.title.trim()) return 'Add a subject for your envelope.';
  if (!draft.recipients.length) return 'Add at least one recipient.';
  const emails = new Set<string>();
  for (const r of draft.recipients) {
    if (!r.name.trim() || !z.email().safeParse(r.email).success)
      return 'Add a name and valid email for every recipient.';
    if (emails.has(r.email.toLowerCase()))
      return 'Each recipient needs a unique email.';
    emails.add(r.email.toLowerCase());
    if (
      !draft.fields.some(
        (f) => f.recipientId === r.id && f.kind === 'signature' && f.required
      )
    )
      return `Place a required signature for ${r.name}.`;
  }
  return undefined;
}
export function signingIssue(
  session: SigningSession,
  values: Record<string, string>,
  consent: boolean
) {
  if (!consent)
    return 'Agree to electronic records and signatures to continue.';
  if (
    session.fields.some(
      (f) => f.required && f.kind !== 'date' && !values[f.id]?.trim()
    )
  )
    return 'Complete every required field before finishing.';
  return undefined;
}
