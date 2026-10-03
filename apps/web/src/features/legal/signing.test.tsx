import { createMemoryHistory, MemoryRouter, Route } from '@solidjs/router';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SigningSession } from './core/models';
import { Signing } from './signing';

vi.mock('@ui', () => ({ Button: (props: any) => <button {...props} /> }));
vi.mock('./components/pdf-document', () => ({ PdfDocument: () => null }));
vi.mock('./queries/source', () => ({
  signingSource: (token: string) => ({
    session: async (): Promise<SigningSession> => ({
      id: 'envelope',
      title: 'Mutual NDA',
      message: '',
      filename: 'NDA.pdf',
      pageCount: 1,
      sourceSha256: 'hash',
      status: 'sent',
      revision: 1,
      expiresAt: null,
      recipient: {
        id: token,
        name: token === 'first' ? 'Alex Morgan' : 'Casey Chen',
        email: `${token}@example.com`,
        order: 1,
        signedAt: null,
        deliveredAt: null,
      },
      canSign: true,
      fields: [
        {
          id: 'signature',
          recipientId: token,
          kind: 'signature',
          page: 1,
          x: 0,
          y: 0,
          width: 0.2,
          height: 0.1,
          required: true,
          value: null,
        },
      ],
    }),
    document: async () => new Uint8Array(),
  }),
}));
afterEach(cleanup);

describe('recipient links in the app router', () => {
  it('resets consent, review and signature state when a different recipient link opens in the same tab', async () => {
    const history = createMemoryHistory();
    history.set({ value: '/app/sign#first', replace: true });
    render(() => (
      <MemoryRouter history={history}>
        <Route path="/app/sign" component={Signing} />
      </MemoryRouter>
    ));
    await screen.findByText(/Hello Alex Morgan/);
    fireEvent.change(
      screen.getByLabelText('I agree to electronic records and signatures'),
      { target: { checked: true } }
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole('button', {
            name: 'Review & sign',
          }) as HTMLButtonElement
        ).disabled
      ).toBe(false)
    );
    fireEvent.click(screen.getByRole('button', { name: 'Review & sign' }));
    fireEvent.input(screen.getByLabelText('Signature field'), {
      target: { value: 'Alex Morgan' },
    });
    history.set({ value: '/app/sign#second', replace: false });
    await screen.findByText(/Hello Casey Chen/);
    expect(
      (
        screen.getByLabelText(
          'I agree to electronic records and signatures'
        ) as HTMLInputElement
      ).checked
    ).toBe(false);
    expect(
      (
        screen.getByRole('button', {
          name: 'Review & sign',
        }) as HTMLButtonElement
      ).disabled
    ).toBe(true);
    fireEvent.change(
      screen.getByLabelText('I agree to electronic records and signatures'),
      { target: { checked: true } }
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole('button', {
            name: 'Review & sign',
          }) as HTMLButtonElement
        ).disabled
      ).toBe(false)
    );
    fireEvent.click(screen.getByRole('button', { name: 'Review & sign' }));
    expect(
      (screen.getByLabelText('Signature field') as HTMLInputElement).value
    ).toBe('');
  });
  it('explains a missing signing capability', () => {
    render(() => (
      <MemoryRouter>
        <Route path="/" component={Signing} />
      </MemoryRouter>
    ));
    expect(
      screen.getByText('This signing link could not be opened.')
    ).toBeTruthy();
  });
});
