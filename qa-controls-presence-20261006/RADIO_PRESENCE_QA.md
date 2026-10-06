# Controls and presence browser QA — 2026-10-06

Local HTTPS35709, own isolated headless Chromium, two authenticated contexts (forms-final-qa@macro.com and forms-presence-qa@macro.com), plus anonymous public respondent. No source/git changes. Retained event demo untouched.

## Controls

Original respondent radios used browser-native appearance with normal color-scheme in dark mode, producing white circles. Builder markers were unaffected.

Shared RadioGroup/Checkbox fix verified in light and dark: hollow 16px ring unchecked, small checked dot, square checkbox, label selection, radio ArrowDown exclusivity, checkbox Space toggle on/off, optional Clear selection reset. Current public dark computed ring border oklch(0.75 0 21), interior oklch(0.17 0 0), width16px. No giant white fill.

Delayed only own scratch POST, then continued actual backend request: final shared Clear selection button kept same DOM identity, x445/y271/width99.734375/height20 before and pending; content container remained606x100. Button disabled pending, actual response saved. No layout jump. Two real scratch submissions saved across owner and second editor; event demo unchanged.

## Presence and sharing

Scratch form01a11273-9918-7c9d-a5f5-0721d8135079.
- Signed-in owner /respond: zero form/database track_entity frames, including Edit my response.
- Editor: form open + database open frames.
- Native Share UI: searched second local account, selected Edit, shared successfully after explicit ShareTrigger identity fix.
- Second browser account editor: form+database open; owner header displayed collaborator avatar in avatar-group.
- Second account Preview: zero tracking and no collaborator avatars.
- Second account actual /respond including real POST: zero tracking.

One error boundary during HMR before ShareTrigger explicit identity fix: '<ShareTrigger> requires an explicit block type'. Hard reload after fix and full share flow passed. No subsequent JavaScript page errors.

Artifacts: radio-dark-final-unchecked.png, radio-dark-final-checked.png, radio-light-final-unchecked.png, radio-light-final-checked.png, radio-dark-public-final.png (latest settled public view), clear-selection-shared-button-pending.png, presence-two-editors.png, presence-respondent-none.png, presence-preview-none.png.
