import { expect, test } from '@playwright/test';

test('offline send survives cache reopen and is sent once after reconnect', async ({
  page,
  context,
}) => {
  const url = `/email-send.html?scope=send-${crypto.randomUUID()}`;
  await page.goto(url);
  await expect(page.locator('html')).toHaveAttribute('data-ready', 'true');
  await context.setOffline(true);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.locator('#status')).toHaveText('Queued');
  await expect(page.locator('#body')).toBeDisabled();
  await expect(page.locator('#requests')).toHaveText('[]');
  await page.getByRole('button', { name: 'Close cache' }).click();
  await context.setOffline(false);
  await page.goto(url);
  await expect(page.locator('#status')).toHaveText('Accepted');
  await expect(page.locator('#requests')).toHaveText('["send"]');
});

test('offline cancel atomically removes an unattempted send', async ({
  page,
  context,
}) => {
  await page.goto(`/email-send.html?scope=cancel-${crypto.randomUUID()}`);
  await expect(page.locator('html')).toHaveAttribute('data-ready', 'true');
  await context.setOffline(true);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.locator('#status')).toHaveText('Queued');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('#status')).toHaveText('Cancelled');
  await expect(page.locator('#body')).toBeEnabled();
  await expect(page.locator('#requests')).toHaveText('[]');
  await context.setOffline(false);
  await expect(page.locator('#requests')).toHaveText('["cancel"]');
});
