import { test, expect } from '@playwright/test';

test('history checkpoints edits and restores without discarding the newer version', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  const body = page.getByRole('textbox', { name: '排版正文' });
  const options = page.locator('[data-options-trigger]');
  const openHistory = async () => {
    await options.click();
    await page.getByRole('button', { name: '历史版本…', exact: true }).click();
    await expect(page.getByRole('heading', { name: '历史版本', exact: true })).toBeVisible();
  };
  await body.fill('旧正文');
  await openHistory();
  await expect(page.locator('.history-preview pre')).toHaveText('旧正文');
  await page.getByRole('button', { name: '关闭历史' }).click();
  await body.fill('新正文');
  await openHistory();
  await expect(page.locator('.history-preview pre')).toHaveText('新正文');
  await page.locator('.history-list button').nth(1).click();
  await expect(page.locator('.history-preview pre')).toHaveText('旧正文');
  await page.getByRole('button', { name: '恢复此版本', exact: true }).click();
  await expect(page.getByRole('button', { name: '确认恢复', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '确认恢复', exact: true }).click();
  await expect(page.locator('dialog')).toHaveCount(0);
  await expect(body).toHaveText('旧正文');
  await openHistory();
  await page.locator('.history-list button').nth(1).click();
  await expect(page.locator('.history-preview pre')).toHaveText('新正文');
});
