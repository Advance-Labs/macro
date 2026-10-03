# Legal and eSignature

Open **Legal** in either desktop sidebar, visit `/app/legal`, or use `g l`.
Legal is a Macro workspace panel, with the same navigation rail, collapsible
workspace sidebar, split controls, and breadcrumbs as Email and Tasks. Shift-click
Legal in the app rail to open it beside another workspace; close its panel with
the shared close control. Envelopes are private to the sending account.

New envelope preparation lives at `/app/legal/new`. An agreement opens at
`/app/legal/<envelope-id>`; direct navigation and reload restore that agreement.
Use the Legal breadcrumb to return to the list. Status filters and search live
in the shared workspace layout.

1. Choose **New envelope** and upload a static PDF (10 MB, up to 100 pages).
2. Add recipients with their names, email addresses, and signing order. Recipients
   with the same order can sign in parallel. Otherwise the next group is invited
   only after all earlier groups finish.
3. Choose a recipient and field type, then click the PDF to place a field. Drag
   fields to move them. Every signer needs a required Signature field. Initials,
   Date signed, and Text fields are also available. Page controls handle longer PDFs.
4. Review the subject and message, then choose **Send for signature**. Sending
   freezes the source, recipients, and fields. **Save & close** preserves a draft.
5. The recipient's email opens `/app/sign#<capability>` without requiring a Macro
   account. Signing capabilities stay in URL fragments and are sent in HTTP
   headers, not query parameters. Do not record or share these capabilities.
6. The signer explicitly consents to electronic records/signatures, reviews the
   agreement, enters their assigned fields, and chooses **Finish signing**.
7. When all recipients finish, both the sender and recipients can download the
   completed PDF with flattened values and its certificate of completion. The
   sender's detail view shows delivery/signing times and document fingerprints.

**Resend invitations** rotates links for active unsigned recipients. Failed
invitations display as undelivered and can be retried. Links expire 30 days after
sending. **Void** and **Decline to sign** require a reason, close the request,
and retain the audit evidence. They cannot change completed agreements.

Supported signatures use typed names/initials with embedded fonts. Static,
unencrypted PDFs are required; rotated/cropped pages, XFA, and active scripts are
rejected. Signature fonts support Latin, Greek, and Cyrillic; unsupported glyphs
are rejected before a signature is committed. Identity evidence is possession of
an emailed link, explicit consent, timestamps, and browser user agent. This first
version does not add identity verification, qualified digital signatures,
templates, bulk sending, or CLM workflows.

For browser verification with new backend code, use the repository's local stack
or the loopback-only `esignature/examples/local_demo.rs` with Postgres and Mailpit.
Never deploy the example: it accepts a fixed local demo credential.
