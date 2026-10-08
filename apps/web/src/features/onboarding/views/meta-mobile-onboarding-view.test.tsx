import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { stubOnboardingBrowser } from '../tests/render';
import { MetaMobileOnboardingView } from './meta-mobile-onboarding-view';

beforeEach(() => {
  sessionStorage.clear();
  stubOnboardingBrowser();
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('meta mobile onboarding', () => {
  it('creates a team from an email and stops on the desktop handoff', async () => {
    const provision = vi.fn(async () => ({ t: 'created' as const }));
    const lead = vi.fn();
    const identify = vi.fn();
    renderView({ provision, lead, identify });

    await screen.findByRole('heading', { name: 'Create your workspace' });
    fireEvent.click(screen.getByRole('radio', { name: 'Coral' }));
    fireEvent.click(screen.getByRole('button', { name: 'Get started' }));

    await screen.findByRole('heading', { name: /Connect your work/ });
    fireEvent.click(screen.getByRole('button', { name: 'Use email instead' }));
    fireEvent.input(screen.getByLabelText('Email address'), {
      target: { value: 'Ada@Acme.com' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    await screen.findByRole('heading', { name: 'Built for teams.' });
    fireEvent.input(screen.getByLabelText('Workspace name'), {
      target: { value: 'Acme' },
    });
    fireEvent.input(screen.getByLabelText('Teammate 1 email'), {
      target: { value: 'grace@acme.com' },
    });
    fireEvent.click(
      screen.getByRole('button', { name: 'Create team & invite' })
    );

    await screen.findByRole('heading', { name: 'Your team has been created.' });
    expect(screen.getByText('Ada@Acme.com'.toLowerCase())).toBeTruthy();
    expect(provision).toHaveBeenCalledWith({
      email: 'ada@acme.com',
      teamName: 'Acme',
      accent: '#f77d67',
      invites: ['grace@acme.com'],
    });
    expect(identify).toHaveBeenCalledWith('ada@acme.com');
    expect(lead).toHaveBeenCalledWith('ada@acme.com');
  });
});

function renderView(options: {
  provision: () => Promise<{ t: 'created' }>;
  lead: (email: string) => void;
  identify: (email: string) => void;
}) {
  return render(() => (
    <MetaMobileOnboardingView
      onGoogle={async () => {}}
      onIdentify={options.identify}
      onLead={options.lead}
      provision={options.provision}
    />
  ));
}
