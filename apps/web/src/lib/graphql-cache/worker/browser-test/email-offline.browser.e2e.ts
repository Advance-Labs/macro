import { expect, test } from '@playwright/test';

for (const native of [false, true]) {
  test(`opens visited and preloaded legacy threads offline (native probe: ${native})`, async ({
    page,
    context,
  }) => {
    await page.goto(`/email-offline.html${native ? '?native' : ''}`);
    await expect(page.locator('#status')).toHaveText('Legacy cache ready', {
      timeout: 60_000,
    });
    await context.setOffline(true);
    try {
      await page.getByRole('button', { name: 'Open visited email' }).click();
      await expect(page.locator('#email')).toHaveText('visited cached body');
      await page.getByRole('button', { name: 'Open preloaded email' }).click();
      await expect(page.locator('#email')).toHaveText('preloaded cached body');
      await page.getByRole('button', { name: 'Open uncached email' }).click();
      await expect(page.locator('#email')).toHaveText(
        'Email unavailable offline'
      );
    } finally {
      await context.setOffline(false);
    }
  });
}
