import { Match, Switch } from 'solid-js';
import { MacroMarkIcon } from './macro-mark-icon';

const CREATING_PAUSE_MS = 1400;

/** How long the creating line stays up so the handoff doesn't flash past. */
function creatingPause(): number {
  if (import.meta.env.VITEST) return 0;
  if (
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  ) {
    return 0;
  }
  return CREATING_PAUSE_MS;
}

/**
 * The beat between "create" and the desktop handoff: the mark settles in the
 * workspace color, then the screen confirms the team and points at desktop.
 */
export function WorkspaceHandoff(props: {
  phase: 'creating' | 'ready';
  accent: string;
  teamName: string;
  email: string;
}) {
  return (
    <div
      class="mx-auto flex w-full max-w-lg flex-col items-center py-6 text-center"
      style={{ '--intro-accent': props.accent }}
    >
      <style>{
        /*css*/ `
        @keyframes meta-workspace-ring {
          0% { transform: scale(.72); opacity: .55; }
          70% { transform: scale(1.15); opacity: 0; }
          100% { transform: scale(1.15); opacity: 0; }
        }
        @keyframes meta-workspace-mark {
          0%, 100% { transform: scale(1); }
          50% { transform: scale(1.06); }
        }
        .meta-workspace-ring { animation: meta-workspace-ring 1.6s cubic-bezier(.16,1,.3,1) infinite; }
        .meta-workspace-mark { animation: meta-workspace-mark 1.6s cubic-bezier(.16,1,.3,1) infinite; }
        @media (prefers-reduced-motion: reduce) {
          .meta-workspace-ring, .meta-workspace-mark { animation: none; }
        }
      `
      }</style>
      <div
        class="relative flex size-28 items-center justify-center"
        aria-hidden="true"
      >
        <div
          class="meta-workspace-ring absolute inset-0 rounded-[28px]"
          style={{
            background:
              'radial-gradient(circle, color-mix(in srgb, var(--intro-accent) 55%, transparent), transparent 70%)',
          }}
        />
        <div
          class="meta-workspace-mark base-header-logo relative flex size-20 items-center justify-center rounded-[22px]"
          style={{
            color: 'var(--intro-accent)',
            background:
              'linear-gradient(145deg, color-mix(in srgb, var(--intro-accent) 7%, #171717), #090909 65%)',
            'box-shadow':
              'inset .5px .5px 1px #ffffff1c, inset -.5px -.5px 1px #0008, 0 1px 1px #131313, 0 4px 10px #0005',
          }}
        >
          <MacroMarkIcon class="h-7 w-10 overflow-visible" />
        </div>
      </div>
      <Switch>
        <Match when={props.phase === 'creating'}>
          <h1
            tabindex="-1"
            class="mt-8 font-[Roboto_Slab_Variable] text-[clamp(1.75rem,8vw,2.25rem)] font-[315] leading-[1.15] tracking-tight outline-none"
          >
            Creating your workspace
          </h1>
          <p class="mt-4 max-w-xs text-sm leading-6 text-ink-muted">
            Setting up {props.teamName || 'your team'}.
          </p>
        </Match>
        <Match when={props.phase === 'ready'}>
          <ReadyCopy teamName={props.teamName} email={props.email} />
        </Match>
      </Switch>
    </div>
  );
}

function ReadyCopy(props: { teamName: string; email: string }) {
  return (
    <div data-workspace-ready>
      <h1
        tabindex="-1"
        class="mt-8 font-[Roboto_Slab_Variable] text-[clamp(1.75rem,8vw,2.25rem)] font-[315] leading-[1.15] tracking-tight outline-none [text-wrap:balance]"
      >
        Your team has been created.
      </h1>
      <p class="mt-4 max-w-sm text-sm leading-6 text-ink-muted [text-wrap:balance]">
        Finish onboarding on desktop. We emailed{' '}
        <span class="text-ink [overflow-wrap:anywhere]">{props.email}</span> a
        link to {props.teamName}. Open it on your computer.
      </p>
    </div>
  );
}

/** Resolves after the creating line has had time to read. */
export function waitForCreatingBeat(): Promise<void> {
  const pause = creatingPause();
  if (pause === 0) return Promise.resolve();
  return new Promise((resolve) => {
    setTimeout(resolve, pause);
  });
}
