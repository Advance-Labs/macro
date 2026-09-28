import { render } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@core/signal/unfurl', () => ({
  useUnfurl: () => [() => undefined],
}));

// Decorators reachable from the renderer open service sockets on import.
vi.mock('@service-connection/websocket', () => ({
  ws: { send() {}, addEventListener() {}, removeEventListener() {} },
  state: () => 'closed',
  createConnectionBlockWebsocketEffect() {},
  createConnectionWebsocketEffect() {},
  parseWebsocketPayload: () => undefined,
}));

vi.mock('@service-storage/websocket', () => ({
  storageWS: { send() {}, addEventListener() {}, removeEventListener() {} },
  createWebSocketJob: () => Promise.reject(new Error('no websocket in tests')),
}));

import { StaticMarkdown } from './StaticMarkdown';

/** The rendered anchors, as `[href, text]`. */
function anchors(markdown: string) {
  const view = render(() => (
    <StaticMarkdown markdown={markdown} target="internal" />
  ));
  return [...view.container.querySelectorAll('a')].map((anchor) => [
    anchor.getAttribute('href'),
    anchor.textContent,
  ]);
}

describe('static markdown links', () => {
  it('renders a bare URL as a link', () => {
    expect(anchors('see https://macro.com/app/channels/abc ok')).toEqual([
      [
        'https://macro.com/app/channels/abc',
        'https://macro.com/app/channels/abc',
      ],
    ]);
  });

  it('renders an internal link node as a link', () => {
    expect(
      anchors(
        '<m-link>{"url":"https://macro.com/app","text":"the channel","title":""}</m-link>'
      )
    ).toEqual([['https://macro.com/app', 'the channel']]);
  });

  it('leaves the surrounding prose in place', () => {
    const view = render(() => (
      <StaticMarkdown
        markdown="Posted in https://macro.com/app, take a look."
        target="internal"
      />
    ));
    expect(view.container.textContent).toBe(
      'Posted in https://macro.com/app, take a look.'
    );
  });
});
