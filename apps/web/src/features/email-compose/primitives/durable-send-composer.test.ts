import { $getRoot } from 'lexical';
import { createSignal } from 'solid-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { message } from '../../email-message/tests/messages';
import type { EmailDraftRestoration } from '../context/compose-capabilities';
import type { ReplyType } from '../core/reply-type';
import { createComposeContext } from '../tests/capabilities';
import { mountEmailComposer } from '../tests/composer';
import { mountReplyComposer } from '../tests/reply';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function restorationContext() {
  const context = createComposeContext();
  let listener: ((event: EmailDraftRestoration) => void) | undefined;
  context.drafts.watchRestorations = (changed) => {
    listener = changed;
    return () => {
      listener = undefined;
    };
  };
  return {
    context,
    restore: (overrides: Partial<EmailDraftRestoration> = {}) =>
      listener?.({
        draftId: 'draft',
        originalDraftId: 'local-draft',
        threadId: 'thread',
        inboxId: 'inbox',
        ...overrides,
      }),
  };
}

it.each([false, true])(
  'restores the mounted standalone body and envelope when the journal unlock is delayed: %s',
  async (delayedUnlock) => {
    const { context, restore } = restorationContext();
    const [locked, setLocked] = createSignal(false);
    context.delivery.sendLocked = locked;
    context.drafts.readDraft = vi.fn(async () => ({
      draft: message('draft', {
        is_draft: true,
        subject: 'Original subject',
        body_text: 'Original body',
        body_html_sanitized: null,
      }),
      persistence: 'queued' as const,
    }));
    const root = mountEmailComposer(context, undefined, {
      draft: message('draft', { is_draft: true }),
    });
    try {
      root.edit('Send-time watermark and stale body', 'Wrong subject');
      setLocked(delayedUnlock);
      restore();
      expect(root.state.context.disabled()).toBe(true);
      if (delayedUnlock) {
        await vi.advanceTimersByTimeAsync(600);
        expect(context.drafts.readDraft).not.toHaveBeenCalled();
        expect(context.drafts.saveDraft).not.toHaveBeenCalled();
        setLocked(false);
      }
      await vi.advanceTimersByTimeAsync(0);
      expect(root.editor.read(() => $getRoot().getTextContent())).toBe(
        'Original body'
      );
      expect(root.state.context.subject()).toBe('Original subject');
      expect(root.state.context.disabled()).toBe(false);
      await vi.advanceTimersByTimeAsync(600);
      expect(context.drafts.saveDraft).not.toHaveBeenCalled();
    } finally {
      root.dispose();
    }
  }
);

it('does not apply a restored draft after its composer unmounts', async () => {
  const { context, restore } = restorationContext();
  const pending =
    Promise.withResolvers<
      Awaited<ReturnType<NonNullable<typeof context.drafts.readDraft>>>
    >();
  context.drafts.readDraft = vi.fn(() => pending.promise);
  const root = mountEmailComposer(context, undefined, {
    draft: message('draft', { is_draft: true }),
  });
  root.edit('Keep current editor');
  restore();
  root.dispose();
  pending.resolve({
    draft: message('draft', {
      is_draft: true,
      body_text: 'Old restoration',
      body_html_sanitized: null,
    }),
    persistence: 'committed',
  });
  await vi.advanceTimersByTimeAsync(0);
  expect(root.editor.read(() => $getRoot().getTextContent())).toBe(
    'Keep current editor'
  );
  expect(context.drafts.saveDraft).not.toHaveBeenCalled();
});

it.each([false, true])(
  'reopens an empty post-send reply with its restored content and attachments when the journal unlock is delayed: %s',
  async (delayedUnlock) => {
    const { context, restore } = restorationContext();
    const [locked, setLocked] = createSignal(delayedUnlock);
    context.delivery.sendLocked = locked;
    const attachment = {
      id: 'upload',
      draft_id: 'draft',
      content_type: 'text/plain',
      file_name: 'original.txt',
      s3_key: 'fixture',
      size: 12,
    };
    context.drafts.readDraft = vi.fn(async () => ({
      draft: message('draft', {
        is_draft: true,
        replying_to_id: 'parent',
        body_text: 'Original reply',
        body_html_sanitized: null,
        attachments_draft: [attachment],
      }),
      persistence: 'queued' as const,
    }));
    const showReply = vi.fn();
    const root = mountReplyComposer(context, undefined, {
      setShowReply: showReply,
    });
    try {
      restore({ replyingToId: 'parent' });
      if (delayedUnlock) {
        await vi.advanceTimersByTimeAsync(600);
        expect(context.drafts.readDraft).not.toHaveBeenCalled();
        expect(context.drafts.saveDraft).not.toHaveBeenCalled();
        expect(showReply).not.toHaveBeenCalled();
        setLocked(false);
      }
      await vi.advanceTimersByTimeAsync(0);
      expect(root.savedDraftId()).toBe('draft');
      expect(root.editor.read(() => $getRoot().getTextContent())).toBe(
        'Original reply'
      );
      expect(root.form.attachments.list()).toEqual([
        expect.objectContaining({
          attachmentId: 'upload',
          fileName: 'original.txt',
        }),
      ]);
      expect(showReply).toHaveBeenCalledWith(true);
      await vi.advanceTimersByTimeAsync(600);
      expect(context.drafts.saveDraft).not.toHaveBeenCalled();
    } finally {
      root.dispose();
    }
  }
);

it('does not replace a newer edited reply with an earlier cancelled send', async () => {
  const { context, restore } = restorationContext();
  context.drafts.readDraft = vi.fn();
  const root = mountReplyComposer(context);
  try {
    root.edit('Newer reply text');
    restore({
      draftId: 'older-draft',
      originalDraftId: 'older-local',
      replyingToId: 'parent',
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(context.drafts.readDraft).not.toHaveBeenCalled();
    expect(root.editor.read(() => $getRoot().getTextContent())).toBe(
      'Newer reply text'
    );
  } finally {
    root.dispose();
  }
});

it.each(['forward', 'reply', 'reply-all'] as const)(
  'consumes external %s requests without changing a queued reply',
  async (type) => {
    const context = createComposeContext();
    const [locked, setLocked] = createSignal(true);
    const [request, setRequest] = createSignal<ReplyType>();
    context.delivery.sendLocked = locked;
    const clear = vi.fn(() => setRequest(undefined));
    const root = mountReplyComposer(
      context,
      undefined,
      { draft: message('draft', { is_draft: true }) },
      { replyRequest: { replyType: request, clear } }
    );
    try {
      const before = {
        type: root.form.replyType(),
        subject: root.form.subject(),
        recipients: JSON.stringify(root.form.recipients()),
      };
      setRequest(type);
      await vi.advanceTimersByTimeAsync(0);
      expect(clear).toHaveBeenCalledOnce();
      expect({
        type: root.form.replyType(),
        subject: root.form.subject(),
        recipients: JSON.stringify(root.form.recipients()),
      }).toEqual(before);
      expect(root.editor.read(() => $getRoot().getTextContent())).toBe(
        'Ready to send'
      );
      setLocked(false);
      await vi.advanceTimersByTimeAsync(600);
      expect(root.form.replyType()).toBe(before.type);
      expect(context.drafts.saveDraft).not.toHaveBeenCalled();
    } finally {
      root.dispose();
    }
  }
);

it('waits for durable storage before applying the initial Forward on a fresh composer', async () => {
  const context = createComposeContext();
  const [locked, setLocked] = createSignal(true);
  const [request, setRequest] = createSignal<ReplyType | undefined>('forward');
  context.delivery.sendLocked = locked;
  const clear = vi.fn(() => setRequest(undefined));
  const root = mountReplyComposer(
    context,
    undefined,
    {},
    { replyRequest: { replyType: request, clear } }
  );
  try {
    expect(clear).not.toHaveBeenCalled();
    setLocked(false);
    await vi.advanceTimersByTimeAsync(0);
    expect(root.form.replyType()).toBe('forward');
    expect(clear).toHaveBeenCalledOnce();
  } finally {
    root.dispose();
  }
});
