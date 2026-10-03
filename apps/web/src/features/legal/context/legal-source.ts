import type {
  Draft,
  Envelope,
  SignatureSubmission,
  SigningSession,
} from '../core/models';
export interface LegalSource {
  list(): Promise<Envelope[]>;
  create(file: File, title: string): Promise<Envelope>;
  update(id: string, draft: Draft): Promise<Envelope>;
  send(id: string, revision: number): Promise<Envelope>;
  resend(id: string): Promise<Envelope>;
  void(id: string, revision: number, reason: string): Promise<Envelope>;
  document(id: string, completed?: boolean): Promise<Uint8Array>;
}
export interface SigningSource {
  session(): Promise<SigningSession>;
  document(completed?: boolean): Promise<Uint8Array>;
  sign(submission: SignatureSubmission): Promise<SigningSession>;
  decline(revision: number, reason: string): Promise<void>;
}
