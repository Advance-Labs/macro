import { For, Match, Show, Switch } from 'solid-js';
import {
  SettingsCard,
  SettingsPage,
  SettingsSection,
} from '../../settings/primitives';
import { CheckboxRow } from '../components/checkbox-row';
import { useCallSettings } from '../context/call-settings-context';
import {
  isHuddleSharingBlocked,
  isRecordingBlocked,
  RECORDING_KINDS,
  recordsByDefault,
  sharesHuddlesByDefault,
} from '../core/call-settings';

const ADMINS_ONLY = 'Only team admins can change this.';

export function CallSettingsView() {
  const calls = useCallSettings();
  const source = calls.createSource();

  return (
    <SettingsPage
      title="Calls"
      description="Choose which calls record on their own and who can see them afterwards."
    >
      <Switch
        fallback={
          <p role="status" class="text-sm text-ink-muted">
            Loading call settings…
          </p>
        }
      >
        <Match when={source.settings()}>
          {(settings) => (
            <>
              <SettingsSection
                title="Record by default"
                description="Calls you start begin recording automatically. Clear every option to keep recording off."
              >
                <SettingsCard>
                  <For each={RECORDING_KINDS}>
                    {(option) => (
                      <CheckboxRow
                        label={option.label}
                        description={option.description}
                        checked={recordsByDefault(settings(), option.kind)}
                        disabled={isRecordingBlocked(settings(), option.kind)}
                        disabledReason="Your team admins have blocked recording these calls."
                        onChange={(value) =>
                          calls.setRecordByDefault(option.kind, value)
                        }
                      />
                    )}
                  </For>
                </SettingsCard>
              </SettingsSection>

              <SettingsSection
                title="Share by default"
                description="Shared calls let your team open the recording, transcript, and AI notes once the call ends. Meetings are never shared with your team."
              >
                <SettingsCard>
                  <CheckboxRow
                    label="Share huddles with my team"
                    description="Huddles you start begin with “Share with team” on. Anyone in the huddle can still turn it off."
                    checked={sharesHuddlesByDefault(settings())}
                    disabled={isHuddleSharingBlocked(settings())}
                    disabledReason="Your team admins have blocked sharing huddles with the team."
                    onChange={(value) => calls.setShareHuddlesByDefault(value)}
                  />
                </SettingsCard>
              </SettingsSection>

              <SettingsSection
                title="1:1 privacy"
                description="Applies to 1:1 meetings and huddles in a two-person direct message, whoever starts them."
              >
                <SettingsCard>
                  <CheckboxRow
                    label="Don't record or transcribe my 1:1s"
                    description="Overrides the other person's settings, and both of you see why it isn't recording. Stops applying once a third teammate or anyone from outside your team joins."
                    checked={settings().refuseOneOnOneRecording}
                    disabled={false}
                    onChange={(value) =>
                      calls.setRefuseOneOnOneRecording(value)
                    }
                  />
                </SettingsCard>
              </SettingsSection>

              <Show when={settings().team}>
                {(team) => (
                  <SettingsSection
                    title="Team policy"
                    description="Block recording or sharing for everyone on your team. Blocks apply whatever each person's settings say."
                    actions={
                      <Show when={!team().canEdit}>
                        <span class="text-xs text-ink-muted">Admins only</span>
                      </Show>
                    }
                  >
                    <SettingsCard>
                      <For each={RECORDING_KINDS}>
                        {(option) => (
                          <CheckboxRow
                            label={`Block recording ${option.label.toLowerCase()}`}
                            description={option.description}
                            checked={team().recordingBlocked[option.kind]}
                            disabled={!team().canEdit}
                            disabledReason={ADMINS_ONLY}
                            onChange={(blocked) =>
                              calls.setRecordingBlocked(option.kind, blocked)
                            }
                          />
                        )}
                      </For>
                      <CheckboxRow
                        label="Block sharing huddles"
                        description="No one's huddles are shared with the team, including huddles already running."
                        checked={team().huddleSharingBlocked}
                        disabled={!team().canEdit}
                        disabledReason={ADMINS_ONLY}
                        onChange={(blocked) =>
                          calls.setHuddleSharingBlocked(blocked)
                        }
                      />
                    </SettingsCard>
                  </SettingsSection>
                )}
              </Show>
            </>
          )}
        </Match>
        <Match when={source.error()}>
          <p role="alert" class="text-sm text-failure">
            Call settings couldn't load. Try again later.
          </p>
        </Match>
      </Switch>
    </SettingsPage>
  );
}
