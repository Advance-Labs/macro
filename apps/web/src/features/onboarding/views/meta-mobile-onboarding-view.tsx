import { createSignal, Match, onCleanup, onMount, Show, Switch } from 'solid-js';
import { ContinueButton, FormInput } from '../components/controls';
import { GoogleAccountsStep } from '../components/google-accounts-step';
import { OnboardingShell } from '../components/onboarding-shell';
import { TeamSetup, TeamSetupFields } from '../components/team-setup';
import {
  WorkspaceHandoff,
  waitForCreatingBeat,
} from '../components/workspace-handoff';
import { WorkspaceIntro } from '../components/workspace-intro';
import {
  type MetaMobileDraft,
  readMetaMobileDraft,
  writeMetaMobileDraft,
} from '../core/meta-mobile-draft';
import {
  isPlausibleEmail,
  removeInviteSlot,
  validInviteEmails,
} from '../core/team';
import {
  WORKSPACE_ACCENTS,
  workspaceAccentNamed,
} from '../core/workspace-accent';
import {
  type ProvisionMobileWorkspaceResult,
  provisionMobileWorkspace,
} from '../queries/provision-mobile-workspace';

type Step = 'theme' | 'account' | 'team' | 'creating' | 'ready';

const failureMessage = (result: ProvisionMobileWorkspaceResult): string => {
  if (result.t === 'invalid') return 'Check the email address and try again.';
  if (result.t === 'rate-limited')
    return 'Too many attempts. Please try again later.';
  if (result.t === 'failed') return result.message;
  return 'Something went wrong. Please try again.';
};

/**
 * Mobile stand-in for desktop onboarding, used when a Meta ad opens in
 * Instagram or Safari. The visitor picks a color, leaves an email or Google
 * account, names the team, and invites teammates. The team is created and a
 * desktop link is emailed; this browser never enters the app.
 */
export function MetaMobileOnboardingView(props: {
  signedInEmail?: string;
  onGoogle: () => Promise<void>;
  onIdentify: (email: string) => void;
  onLead: (email: string) => void;
  provision?: typeof provisionMobileWorkspace;
}) {
  const stored = readMetaMobileDraft();
  const provision = props.provision ?? provisionMobileWorkspace;
  const [step, setStep] = createSignal<Step>(
    initialStep(stored, props.signedInEmail)
  );
  const [accentName, setAccentName] = createSignal(accentNameFrom(stored));
  const [email, setEmail] = createSignal(
    (props.signedInEmail ?? stored?.email ?? '').toLowerCase()
  );
  const [showingEmail, setShowingEmail] = createSignal(
    !props.signedInEmail && !!stored?.email
  );
  const [teamName, setTeamName] = createSignal(stored?.teamName ?? '');
  const [slots, setSlots] = createSignal<string[]>(
    stored?.invites.length ? [...stored.invites, ''] : ['', '']
  );
  const [error, setError] = createSignal<string>();
  const [connecting, setConnecting] = createSignal(false);
  const accent = () => workspaceAccentNamed(accentName());
  const invites = () =>
    validInviteEmails(
      slots().map((slot) => slot.trim().toLowerCase()),
      email().trim().toLowerCase()
    );

  const draft = (ready: boolean): MetaMobileDraft => ({
    accent: accent().color,
    teamName: teamName().trim(),
    email: email().trim().toLowerCase(),
    invites: invites(),
    ready,
  });

  const goTo = (next: Step) => {
    setError(undefined);
    setStep(next);
    writeMetaMobileDraft(draft(next === 'ready'));
    queueMicrotask(() => {
      document
        .querySelector<HTMLElement>('.onboarding-flow h1')
        ?.focus({ preventScroll: true });
    });
  };

  const connectGoogle = () => {
    if (connecting()) return;
    setConnecting(true);
    writeMetaMobileDraft(draft(false));
    void props.onGoogle().finally(() => setConnecting(false));
  };

  const submitEmail = () => {
    const address = email().trim().toLowerCase();
    if (!isPlausibleEmail(address)) {
      setError('Enter a valid email address.');
      return;
    }
    setEmail(address);
    props.onIdentify(address);
    goTo('team');
  };

  const create = () => {
    const address = (props.signedInEmail ?? email()).trim().toLowerCase();
    if (!isPlausibleEmail(address) || !teamName().trim()) return;
    setEmail(address);
    goTo('creating');
  };

  return (
    <OnboardingShell
      wide
      onBack={
        backTarget(step(), showingEmail(), !!props.signedInEmail)
          ? () => {
              const target = backTarget(
                step(),
                showingEmail(),
                !!props.signedInEmail
              );
              if (target === 'google') {
                setShowingEmail(false);
                setError(undefined);
                return;
              }
              if (target) goTo(target);
            }
          : undefined
      }
    >
      <Switch>
        <Match when={step() === 'theme'}>
          <WorkspaceIntro
            accent={accent()}
            onSelectAccent={setAccentName}
            onContinue={() => goTo('account')}
          />
        </Match>
        <Match when={step() === 'account'}>
          <Show
            when={showingEmail()}
            fallback={
              <>
                <GoogleAccountsStep
                  mode="work"
                  connecting={connecting() ? 'work' : undefined}
                  error={error()}
                  onConnectWork={connectGoogle}
                  onConnectPersonal={() => {}}
                />
                <button
                  type="button"
                  class="mx-auto mt-2 block rounded-lg px-4 py-2 text-sm text-ink-muted hover:text-ink focus-visible:outline-2 focus-visible:outline-ink"
                  onClick={() => {
                    setError(undefined);
                    setShowingEmail(true);
                  }}
                >
                  Use email instead
                </button>
              </>
            }
          >
            <form
              class="mx-auto flex w-full max-w-sm flex-col gap-8"
              onSubmit={(event) => {
                event.preventDefault();
                submitEmail();
              }}
            >
              <h1
                tabindex="-1"
                class="text-center font-[Roboto_Slab_Variable] text-[clamp(1.75rem,8vw,2.25rem)] font-[315] leading-tight tracking-tight outline-none [text-wrap:balance]"
              >
                Sign up with your work email.
              </h1>
              <FormInput
                id="meta-email"
                type="email"
                label="Email address"
                placeholder="name@company.com"
                value={email()}
                autoFocus
                invalid={!!error()}
                onInput={(value) => {
                  setEmail(value);
                  setError(undefined);
                }}
              />
              <Show when={error()}>
                <p
                  role="alert"
                  class="-mt-4 text-center text-sm text-ink-muted"
                >
                  {error()}
                </p>
              </Show>
              <ContinueButton label="Continue" onClick={submitEmail} />
              <p class="text-center text-xs leading-5 text-ink-extra-muted">
                Your work email becomes your Macro sign-in. You’ll finish
                connecting Google on your computer. By continuing, you agree to
                our{' '}
                <a href="/terms" class="underline underline-offset-2">
                  terms
                </a>{' '}
                and{' '}
                <a href="/privacy" class="underline underline-offset-2">
                  privacy policy
                </a>
                .
              </p>
            </form>
          </Show>
        </Match>
        <Match when={step() === 'team'}>
          <TeamSetup>
            <TeamSetupFields
              id="meta-team"
              name={teamName()}
              emails={slots()}
              onNameChange={setTeamName}
              onEmailChange={(index, value) =>
                setSlots((current) =>
                  current.map((slot, i) => (i === index ? value : slot))
                )
              }
              onRemoveEmail={(index) =>
                setSlots((current) => removeInviteSlot(current, index))
              }
              onAddEmail={() => setSlots((current) => [...current, ''])}
            />
            <Show when={error()}>
              <p role="alert" class="mt-4 text-center text-sm text-ink-muted">
                {error()}
              </p>
            </Show>
            <ContinueButton
              label={
                invites().length > 0 ? 'Create team & invite' : 'Create team'
              }
              disabled={!teamName().trim()}
              onClick={create}
            />
          </TeamSetup>
        </Match>
        <Match when={step() === 'creating' || step() === 'ready'}>
          <CreatingStep
            phase={step() === 'ready' ? 'ready' : 'creating'}
            accent={accent().color}
            teamName={teamName().trim()}
            email={email()}
            invites={invites()}
            provision={provision}
            onLead={props.onLead}
            onReady={() => {
              writeMetaMobileDraft(draft(true));
              setStep('ready');
            }}
            onError={(message) => {
              setError(message);
              setStep('team');
            }}
            onSso={() => {
              setShowingEmail(false);
              setError(
                'This email signs in with Google. Continue with Google, then finish on your computer.'
              );
              setStep('account');
            }}
          />
        </Match>
      </Switch>
    </OnboardingShell>
  );
}

function CreatingStep(props: {
  phase: 'creating' | 'ready';
  accent: string;
  teamName: string;
  email: string;
  invites: string[];
  provision: typeof provisionMobileWorkspace;
  onLead: (email: string) => void;
  onReady: () => void;
  onError: (message: string) => void;
  onSso: () => void;
}) {
  onMount(() => {
    let disposed = false;
    onCleanup(() => {
      disposed = true;
    });
    if (props.phase === 'ready') return;
    const input = {
      email: props.email,
      teamName: props.teamName,
      accent: props.accent,
      invites: [...props.invites],
    };
    const finish = {
      provision: props.provision,
      onLead: props.onLead,
      onReady: props.onReady,
      onSso: props.onSso,
      onError: props.onError,
    };
    void (async () => {
      const [result] = await Promise.all([
        finish.provision(input),
        waitForCreatingBeat(),
      ]);
      if (disposed) return;
      if (result.t === 'created') {
        finish.onLead(input.email);
        finish.onReady();
        return;
      }
      if (result.t === 'sso-required') {
        finish.onSso();
        return;
      }
      finish.onError(failureMessage(result));
    })();
  });
  return (
    <WorkspaceHandoff
      phase={props.phase}
      accent={props.accent}
      teamName={props.teamName}
      email={props.email}
    />
  );
}

function initialStep(
  stored: MetaMobileDraft | undefined,
  signedInEmail: string | undefined
): Step {
  if (stored?.ready) return 'ready';
  if (signedInEmail && stored?.accent) return 'team';
  return 'theme';
}

function accentNameFrom(stored: MetaMobileDraft | undefined): string {
  return (
    WORKSPACE_ACCENTS.find(
      (item) => item.color.toLowerCase() === stored?.accent.toLowerCase()
    )?.name ?? 'Mint'
  );
}

function backTarget(
  step: Step,
  showingEmail: boolean,
  signedIn: boolean
): Step | 'google' | undefined {
  if (step === 'account' && showingEmail) return 'google';
  if (step === 'account') return 'theme';
  if (step === 'team') return signedIn ? 'theme' : 'account';
  return undefined;
}
