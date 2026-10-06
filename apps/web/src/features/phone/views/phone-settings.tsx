import {
  SettingsButton,
  SettingsCard,
  SettingsPage,
  SettingsRow,
  SettingsSection,
} from '@app/features/settings/primitives';
import PhoneIcon from '@phosphor/phone.svg';
import { ToggleSwitch } from '@ui';
import { createSignal, For, Show } from 'solid-js';
import type {
  PhonePlanSource,
  PhoneSettingsSource,
} from '../context/phone-context';
import { formatPhoneNumber } from '../core/phone-call';
import {
  canToggleAddon,
  minutesUsage,
  type PhonePlan,
  type PhonePlanSeat,
  seatName,
  seatPhoneStatus,
} from '../core/phone-plan';

/** The viewer's phone setup: their plan, numbers, caller id, and recording. */
export function PhoneSettingsView(props: {
  settings: PhoneSettingsSource;
  plan?: PhonePlanSource;
  onOpenDialer: () => void;
}) {
  const settings = () => props.settings.settings();
  const dialingEnabled = () => settings()?.dialingEnabled === true;
  const callerId = () => {
    const number = settings()?.callerId;
    return number ? formatPhoneNumber(number) : undefined;
  };

  return (
    <SettingsPage
      title="Phone"
      description="Call phone numbers from Macro and take calls to your number. Phone calls are recorded, transcribed, and summarized like any other call."
    >
      <Show when={props.plan}>
        {(plan) => <PhonePlanSection source={plan()} />}
      </Show>
      <SettingsSection title="Your numbers">
        <SettingsCard>
          <Show
            when={settings()}
            fallback={
              <SettingsRow
                label={
                  props.settings.isError()
                    ? 'Could not load your phone settings'
                    : 'Loading…'
                }
              />
            }
          >
            {(loaded) => (
              <Show
                when={loaded().phoneNumbers.length > 0}
                fallback={
                  <SettingsRow
                    label="No number yet"
                    description="Ask your workspace admin to assign you a number so people can call you in Macro."
                  />
                }
              >
                <For each={loaded().phoneNumbers}>
                  {(number, index) => (
                    <SettingsRow
                      label={
                        <span class="tabular-nums">
                          {formatPhoneNumber(number)}
                        </span>
                      }
                      description={
                        index() === 0
                          ? 'Calls to this number ring you here. People you call see it.'
                          : 'Calls to this number ring you here.'
                      }
                    />
                  )}
                </For>
              </Show>
            )}
          </Show>
        </SettingsCard>
      </SettingsSection>
      <SettingsSection title="Calling out">
        <SettingsCard>
          <SettingsRow
            label="Outbound calls"
            description={
              dialingEnabled()
                ? callerId()
                  ? `People you call see ${callerId()}.`
                  : 'People you call see your workspace number.'
                : "Phone calling isn't set up for your workspace yet."
            }
            stackOnNarrow
          >
            <SettingsButton
              variant="strong"
              disabled={!dialingEnabled()}
              onClick={() => props.onOpenDialer()}
            >
              <PhoneIcon class="size-4" />
              Open dialer
            </SettingsButton>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>
      <SettingsSection title="Recording">
        <SettingsCard>
          <SettingsRow
            label="Calls are recorded and transcribed"
            description="Phone calls appear in Calls with a recording, transcript, and summary, and are linked to the CRM contact you spoke with. Tell the other party when the law where they are requires it."
          />
        </SettingsCard>
      </SettingsSection>
    </SettingsPage>
  );
}

/** Whether the viewer can call, their minutes, and the Phone add-on. */
function PhonePlanSection(props: { source: PhonePlanSource }) {
  const [error, setError] = createSignal<string>();

  async function toggle(seat: PhonePlanSeat, enabled: boolean) {
    setError(undefined);
    try {
      await props.source.setAddon(seat.userId, enabled);
    } catch (failure) {
      setError(
        failure instanceof Error
          ? failure.message
          : 'Could not change the Phone add-on. Try again.'
      );
    }
  }

  return (
    <SettingsSection title="Plan">
      <SettingsCard>
        <Show
          when={props.source.plan()}
          fallback={
            <SettingsRow
              label={
                props.source.isError()
                  ? 'Could not load your phone plan'
                  : 'Loading…'
              }
            />
          }
        >
          {(plan) => (
            <>
              <OwnSeatRow plan={plan()} />
              <For each={plan().seats}>
                {(seat, index) => (
                  <Show when={plan().canManage}>
                    <SettingsRow
                      label={
                        index() === 0 ? 'Your seat' : seatName(seat.userId)
                      }
                      description={seatPhoneStatus(seat, plan())}
                      stackOnNarrow
                    >
                      <Show when={canToggleAddon(seat, plan())}>
                        <ToggleSwitch
                          size="md"
                          label={
                            <span class="sr-only">
                              Phone add-on for{' '}
                              {index() === 0
                                ? 'your seat'
                                : seatName(seat.userId)}
                            </span>
                          }
                          checked={seat.addon}
                          disabled={props.source.pendingSeat() !== null}
                          onChange={(checked) => void toggle(seat, checked)}
                        />
                      </Show>
                    </SettingsRow>
                  </Show>
                )}
              </For>
              <Show when={error()}>
                <SettingsRow
                  label={
                    <span role="alert" class="text-failure">
                      {error()}
                    </span>
                  }
                />
              </Show>
            </>
          )}
        </Show>
      </SettingsCard>
    </SettingsSection>
  );
}

/** The viewer's own calling: on or off, minutes, and why it is blocked. */
function OwnSeatRow(props: { plan: PhonePlan }) {
  const own = () => props.plan.seats[0];
  const description = () => {
    if (props.plan.blockedMessage) return props.plan.blockedMessage;
    const minutes = props.plan.minutes;
    if (minutes)
      return `${minutesUsage(minutes)} this period. Extra minutes are billed as usage.`;
    if (own()?.addon) return 'Your seat has the Phone add-on.';
    if (own()?.phoneEnabled) return 'Phone calls are included with your plan.';
    return props.plan.canManage
      ? 'Phone calls need the Phone add-on or a Max plan.'
      : 'Phone calls need the Phone add-on. Ask whoever manages your plan to add it to your seat.';
  };
  return (
    <SettingsRow
      label={
        own()?.phoneEnabled ? 'Phone calling is on' : 'Phone calling is off'
      }
      description={description()}
    />
  );
}
