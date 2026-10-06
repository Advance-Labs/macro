import {
  SettingsButton,
  SettingsCard,
  SettingsPage,
  SettingsRow,
  SettingsSection,
} from '@app/features/settings/primitives';
import PhoneIcon from '@phosphor/phone.svg';
import { For, Show } from 'solid-js';
import type { PhoneSettingsSource } from '../context/phone-context';
import { formatPhoneNumber } from '../core/phone-call';

/** The viewer's phone setup: their numbers, caller id, and recording. */
export function PhoneSettingsView(props: {
  settings: PhoneSettingsSource;
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
