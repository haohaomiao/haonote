import { test, expect, type Page } from '@playwright/test';

const original = '已经保存的正文';
async function openSaved(page: Page) {
  await page.addInitScript(() => {
    if (localStorage.getItem('qingnote-development-preview')) return;
    localStorage.setItem(
      'qingnote-development-preview',
      JSON.stringify([
        {
          id: 'undo-note',
          headId: 'original',
          content: {
            text: '已经保存的正文',
            title: '撤销测试',
            color: 'butter',
            deleted: false,
            archived: false,
          },
          createdAt: '2026-09-21T00:00:00Z',
          updatedAt: '2026-09-21T00:00:00Z',
          conflicts: [],
          pending: false,
        },
      ]),
    );
  });
  await page.goto('/');
  await page
    .getByRole('button', { name: /打开便签：/ })
    .first()
    .click();
  return page.getByRole('textbox', { name: '排版正文' });
}

test('opening and switching modes never makes saved content undoable', async ({ page }) => {
  const body = await openSaved(page);
  await expect(body).toHaveText(original);
  for (const key of ['Control+z', 'Control+y', 'Control+Shift+z', 'Control+z']) {
    await body.press(key);
    await expect(body).toHaveText(original);
  }
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  await page.locator('textarea').press('Control+z');
  await expect(page.locator('textarea')).toHaveValue(original);
  await page.getByRole('button', { name: '切换到排版编辑' }).click();
  await body.press('Control+z');
  await expect(body).toHaveText(original);
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await page.reload();
  await page
    .getByRole('button', { name: /打开便签：/ })
    .first()
    .click();
  await body.press('Control+z');
  await expect(body).toHaveText(original);
});

test('typing, undo, redo and clipboard shortcuts preserve the saved baseline', async ({
  page,
  context,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const body = await openSaved(page);
  await body.press('Control+End');
  await body.pressSequentially('abc');
  await expect(body).toHaveText(original + 'abc');
  await body.press('Control+z');
  await expect(body).toHaveText(original);
  await body.press('Control+y');
  await expect(body).toHaveText(original + 'abc');
  await body.press('Control+a');
  await body.press('Control+c');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(original + 'abc');
  await body.press('Control+x');
  await expect(body).toHaveText('');
  await body.press('Control+z');
  await expect(body).toHaveText(original + 'abc');
  await body.press('Control+y');
  await expect(body).toHaveText('');
  await body.press('Control+v');
  await expect(body).toHaveText(original + 'abc');
});

test('source editor uses standard undo redo and clipboard shortcuts', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openSaved(page);
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  const source = page.locator('textarea');
  await source.press('Control+End');
  await source.pressSequentially('abc');
  await source.press('Control+z');
  await expect(source).toHaveValue(original);
  await source.press('Control+y');
  await expect(source).toHaveValue(original + 'abc');
  await source.press('Control+a');
  await source.press('Control+c');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(original + 'abc');
  await source.press('Control+x');
  await expect(source).toHaveValue('');
  await source.press('Control+z');
  await expect(source).toHaveValue(original + 'abc');
  await source.press('Control+y');
  await expect(source).toHaveValue('');
  await source.press('Control+v');
  await expect(source).toHaveValue(original + 'abc');
});

test('external refresh clears stale undo and redo without making the new version undoable', async ({
  page,
}) => {
  const body = await openSaved(page);
  await body.press('Control+End');
  await body.pressSequentially('abc');
  await expect
    .poll(() =>
      page.evaluate(
        () => JSON.parse(localStorage.getItem('qingnote-development-preview')!)[0].content.text,
      ),
    )
    .toBe(original + 'abc');
  await body.press('Control+z');
  await expect
    .poll(() =>
      page.evaluate(
        () => JSON.parse(localStorage.getItem('qingnote-development-preview')!)[0].content.text,
      ),
    )
    .toBe(original);
  await page.evaluate(async () => {
    const api = await import('/src/api.ts');
    const [note] = await api.listNotes();
    await api.saveNote(note, { ...note.content, text: '另一台设备的新正文' });
  });
  await expect(body).toHaveText('另一台设备的新正文');
  for (const key of ['Control+y', 'Control+z', 'Control+Shift+z']) {
    await body.press(key);
    await expect(body).toHaveText('另一台设备的新正文');
  }
});
