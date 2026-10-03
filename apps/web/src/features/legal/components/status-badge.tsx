import { type Envelope, isExpired, statusLabels } from '../core/models';
export function StatusBadge(props: {
  envelope: Pick<Envelope, 'status' | 'expiresAt'>;
}) {
  return (
    <span
      class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium bg-ink/5 text-ink-muted"
      classList={{
        'text-success bg-success/10': props.envelope.status === 'completed',
        'text-accent bg-accent-bg':
          props.envelope.status === 'sent' && !isExpired(props.envelope),
        'text-failure bg-failure/10':
          props.envelope.status === 'declined' || isExpired(props.envelope),
      }}
    >
      <span class="size-1.5 rounded-full bg-current" />
      {isExpired(props.envelope)
        ? 'Expired'
        : statusLabels[props.envelope.status]}
    </span>
  );
}
