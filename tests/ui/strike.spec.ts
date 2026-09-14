import { test, expect } from '@playwright/test';

test('strikethrough menu, shortcut and Markdown persist and toggle off', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  const body = page.getByRole('textbox', { name: '排版正文' });
  await body.fill('已完成事项');
  await body.selectText();
  await body.click({ button: 'right', position: { x: 30, y: 20 } });
  await page.getByRole('button', { name: '删除线', exact: true }).click();
  await expect(body.locator('s')).toHaveText('已完成事项');
  await page.keyboard.press('Control+z');
  await expect(body.locator('s')).toHaveCount(0);
  await body.selectText();
  await page.keyboard.press('Control+Shift+s');
  await expect(body.locator('s')).toHaveText('已完成事项');
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  const source = page.locator('textarea');
  await expect(source).toHaveValue('~~已完成事项~~');
  await source.evaluate((element) => {
    element.focus();
    element.setSelectionRange(2, element.value.length - 2);
  });
  await page.keyboard.press('Control+Shift+s');
  await expect(source).toHaveValue('已完成事项');
  await page.keyboard.press('Control+Shift+s');
  await expect(source).toHaveValue('~~已完成事项~~');
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await page.reload();
  await page
    .getByRole('button', { name: /打开便签：/ })
    .first()
    .click();
  await page.getByRole('button', { name: '切换到排版编辑' }).click();
  await expect(body.locator('s')).toHaveText('已完成事项');
  await body.selectText();
  await body.click({ button: 'right', position: { x: 30, y: 20 } });
  await page.getByRole('button', { name: '删除线', exact: true }).click();
  await expect(body.locator('s')).toHaveCount(0);
});
