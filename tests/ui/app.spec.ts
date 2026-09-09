import { test, expect } from '@playwright/test';
import type { Editor } from '@tiptap/core';

test('library views combine time sorting, filters and title/body search', async ({ page }) => {
  const now = Date.parse('2026-09-08T12:00:00Z');
  await page.clock.setFixedTime(now);
  const seed = [
    { id: 'a', title: 'Alpha 计划', text: '预算 苹果', created: 60, updated: 0 },
    { id: 'b', title: '新便签', text: '会议 banana', created: 1, updated: 1 },
    { id: 'c', title: '中间便签', text: 'Alpha 梨', created: 10, updated: 5 },
    { id: 'd', title: '归档便签', text: '苹果', created: 0, updated: 0, archived: true },
  ].map((item) => ({
    id: item.id,
    headId: item.id,
    pending: true,
    conflicts: [],
    createdAt: new Date(now - item.created * 86400000).toISOString(),
    updatedAt: new Date(now - item.updated * 86400000).toISOString(),
    content: {
      title: item.title,
      text: item.text,
      color: 'butter',
      archived: !!item.archived,
      deleted: false,
    },
  }));
  await page.addInitScript(
    (notes) => localStorage.setItem('qingnote-development-preview', JSON.stringify(notes)),
    seed,
  );
  await page.goto('/');
  const titles = page.locator('.note-card h2');
  await expect(titles).toHaveText(['Alpha 计划', '新便签', '中间便签']);
  await page.getByRole('button', { name: '列表视图', exact: true }).click();
  await expect(page.locator('.note-list')).toBeVisible();
  await page.getByLabel('时间排序').selectOption('created-asc');
  await expect(titles).toHaveText(['Alpha 计划', '中间便签', '新便签']);
  await page.getByLabel('时间排序').selectOption('created-desc');
  await expect(titles).toHaveText(['新便签', '中间便签', 'Alpha 计划']);
  await page.getByLabel('时间筛选').selectOption('week');
  await expect(titles).toHaveText(['新便签']);
  await page.reload();
  await expect(page.getByRole('button', { name: '列表视图', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByLabel('时间排序')).toHaveValue('created-desc');
  await expect(page.getByLabel('时间筛选')).toHaveValue('week');
  await expect(titles).toHaveText(['新便签']);
  await page.getByLabel('时间排序').selectOption('updated-desc');
  await page.getByLabel('时间筛选').selectOption('today');
  await expect(titles).toHaveText(['Alpha 计划']);
  await page.keyboard.press('Control+f');
  const search = page.getByRole('textbox', { name: '搜索便签' });
  await expect(search).toBeFocused();
  await search.fill('  ALPHA 苹果  ');
  await expect(titles).toHaveText(['Alpha 计划']);
  await search.fill('banana');
  await expect(titles).toHaveCount(0);
  await expect(page.getByText('没有找到这张便签', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '写下第一张便签' })).toHaveCount(0);
  await page.getByRole('button', { name: '清除搜索和筛选' }).click();
  await expect(titles).toHaveCount(3);
  await search.fill('苹果');
  await page.locator('nav button').filter({ hasText: '归档' }).click();
  await expect(titles).toHaveText(['归档便签']);
  await page.getByRole('button', { name: '清除搜索和筛选' }).click();
  await page.locator('nav button').filter({ hasText: '我的便签' }).click();
  await page.screenshot({ path: 'artifacts/list-view.png' });
  await page.getByRole('button', { name: '卡片视图', exact: true }).click();
  await expect(page.locator('.note-list')).toHaveCount(0);
  await expect(titles).toHaveCount(3);
});

test('clipboard menu and system shortcuts preserve rich text and undo', async ({
  page,
  context,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  const rich = page.getByRole('textbox', { name: '排版正文' });
  await rich.fill('剪贴板测试');
  await rich.press('Control+a');
  await rich.press('Control+b');
  await rich.click({ button: 'right' });
  await page.getByRole('button', { name: '复制', exact: true }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('剪贴板测试');
  await rich.press('End');
  await rich.click({ button: 'right' });
  await page.getByRole('button', { name: '粘贴', exact: true }).click();
  await expect(rich).toHaveText('剪贴板测试剪贴板测试');
  await rich.press('Control+z');
  await expect(rich).toHaveText('剪贴板测试');
  await rich.press('Control+a');
  await rich.click({ button: 'right' });
  await page.getByRole('button', { name: '剪切', exact: true }).click();
  await expect(rich).toHaveText('');
  await rich.press('Control+v');
  await expect(rich.locator('strong')).toHaveText('剪贴板测试');
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  const raw = page.getByRole('textbox', { name: '便签内容' });
  await raw.selectText();
  await raw.press('Control+x');
  await expect(raw).toHaveValue('');
  await raw.press('Control+v');
  await expect(raw).toHaveValue('**剪贴板测试**');
});

test('dragging a rich selection moves it once and can be undone', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  const rich = page.getByRole('textbox', { name: '排版正文' });
  await rich.fill('ABC def');
  await rich.evaluate((el) => {
    // This test targets drag/drop, not asynchronous browser selectionchange.
    // Set its starting selection synchronously through the real editor, then
    // send drag events through the normal DOM handlers. Keyboard selection is
    // covered separately by the clipboard and formatting tests.
    const editor = (el as HTMLElement & { editor: Editor }).editor;
    editor.commands.setTextSelection({ from: 1, to: 4 });
    const text = el.querySelector('p')!.firstChild!;
    const from = document.createRange();
    from.setStart(text, 1);
    from.setEnd(text, 2);
    const start = from.getBoundingClientRect();
    const end = document.createRange();
    end.setStart(text, 7);
    end.collapse(true);
    const target = end.getBoundingClientRect();
    const dataTransfer = new DataTransfer();
    el.dispatchEvent(
      new DragEvent('dragstart', {
        bubbles: true,
        cancelable: true,
        dataTransfer,
        clientX: start.x,
        clientY: start.y + 5,
      }),
    );
    el.dispatchEvent(
      new DragEvent('drop', {
        bubbles: true,
        cancelable: true,
        dataTransfer,
        clientX: target.x,
        clientY: target.y + 5,
      }),
    );
    el.dispatchEvent(new DragEvent('dragend', { bubbles: true, dataTransfer }));
  });
  await expect(rich).toHaveText(' defABC');
  await rich.press('Control+z');
  await expect(rich).toHaveText('ABC def');
});

test('editable Markdown preserves tables, tasks, code, links and source on a no-op switch', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  const source =
    '# 计划\n\n- [ ] 买花\n- [x] 看书\n\n| 日期 | 计划 |\n| --- | --- |\n| 周六 | 散步 |\n\n```js\nconst count = 1;\n```\n\n[链接](https://example.com)\n\n<u>下划线</u>\n\n尾段';
  const raw = page.getByRole('textbox', { name: '便签内容' });
  await raw.fill(source);
  await page.getByRole('button', { name: '切换到排版编辑' }).click();
  const rich = page.getByRole('textbox', { name: '排版正文' });
  await expect(rich).toBeVisible();
  await expect(page.locator('.rich-warning')).toHaveCount(0);
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  await expect(raw).toHaveValue(source);
  await page.getByRole('button', { name: '切换到排版编辑' }).click();
  await rich.locator('input[type=checkbox]').first().check();
  await rich.locator('p').last().click();
  await page.keyboard.press('End');
  await page.keyboard.insertText('补充');
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  const updated = await raw.inputValue();
  expect(updated).toContain('[x] 买花');
  expect(updated).toContain('const count = 1;');
  expect(updated).toContain('https://example.com');
  expect(updated).toContain('<u>下划线</u>');
  expect(updated).toContain('尾段补充');
  await page.getByRole('button', { name: '切换到排版编辑' }).click();
  await expect(rich.locator('table')).toContainText('周六');
  await expect(rich.locator('h1')).toHaveText('计划');
});

test('rich paste is inert and IME-style input survives immediate close and reopen', async ({
  page,
}) => {
  const remote: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('tracker.invalid')) remote.push(request.url());
  });
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  const rich = page.getByRole('textbox', { name: '排版正文' });
  await rich.click();
  await rich.evaluate((el) => {
    const clipboardData = new DataTransfer();
    clipboardData.setData(
      'text/html',
      '<p><strong>粘贴文字</strong><img src="https://tracker.invalid/pixel"><script>window.pasteAttack=true</script></p>',
    );
    el.dispatchEvent(
      new ClipboardEvent('paste', { bubbles: true, cancelable: true, clipboardData }),
    );
  });
  await expect(rich.locator('strong')).toHaveText('粘贴文字');
  await expect(rich.locator('img,script')).toHaveCount(0);
  expect(remote).toEqual([]);
  await rich.dispatchEvent('compositionstart');
  await page.keyboard.insertText('中文输入');
  await rich.dispatchEvent('compositionend');
  await expect(rich).toContainText('中文输入');
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await page.locator('.card-content').click();
  await expect(rich).toContainText('中文输入');
  await expect(rich.locator('strong')).toContainText('粘贴文字');
});

test('outside click closes options; independent titles and formatting persist', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  const body = page.getByRole('textbox', { name: '排版正文' });
  await expect(page.getByRole('toolbar', { name: '正文格式工具' })).toHaveCount(0);
  await body.fill('正文不会变成标题');
  await expect(page.getByRole('button', { name: '折叠便签主体', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '便签选项' }).click();
  await page.getByRole('textbox', { name: '便签标题', exact: true }).fill('独立标题');
  await expect(page.locator('.editor-options')).toBeVisible();
  await page.getByRole('button', { name: '便签标题栏' }).click();
  await expect(page.locator('.editor-options')).toHaveCount(0);
  await expect(body).toHaveText('正文不会变成标题');
  await page.keyboard.press('F2');
  await expect(page.getByRole('textbox', { name: '便签标题', exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.locator('.editor-options')).toHaveCount(0);
  await body.fill('格式文字');
  await body.selectText();
  await body.click({ button: 'right', position: { x: 30, y: 20 } });
  await page.getByRole('button', { name: '加粗', exact: true }).click();
  await expect(body.locator('strong')).toHaveText('格式文字');
  await page.keyboard.press('Control+z');
  await expect(body.locator('strong')).toHaveCount(0);
  await body.selectText();
  await body.click({ button: 'right', position: { x: 30, y: 20 } });
  await page.getByRole('button', { name: '加粗', exact: true }).click();
  await body.click({ button: 'right', position: { x: 30, y: 20 } });
  await page.getByRole('button', { name: '斜体', exact: true }).click();
  await body.click({ button: 'right', position: { x: 30, y: 20 } });
  await page.getByRole('button', { name: '下划线', exact: true }).click();
  await expect(body.locator('strong em u')).toHaveText('格式文字');
  await page.getByRole('button', { name: '便签选项' }).click();
  await page.getByRole('combobox', { name: '正文字体' }).selectOption('serif');
  await page.getByRole('combobox', { name: '正文字号' }).selectOption('24');
  await expect(body).toHaveCSS('font-size', '24px');
  await page.getByRole('button', { name: '便签选项' }).click();
  await expect(page.locator('.markdown-preview strong em u')).toHaveText('格式文字');
  await page.screenshot({ path: 'artifacts/text-tools.png' });
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await page.getByRole('textbox', { name: '搜索便签' }).fill('独立标题');
  await expect(page.locator('.note-card')).toHaveCount(1);
  await page.getByRole('button', { name: '打开便签：独立标题' }).click();
  await page.getByRole('button', { name: '便签选项' }).click();
  await expect(page.getByRole('combobox', { name: '正文字体' })).toHaveValue('serif');
  await expect(page.locator('.markdown-preview')).toHaveCSS('font-size', '24px');
  await expect(page.getByRole('button', { name: '便签标题栏' })).toHaveText('独立标题');
});

test('collapse, opacity and safe Markdown preview preserve the source', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  const source =
    '# 周末计划\n\n- [x] **买花**\n- [ ] 看书\n\n```js\nconst n = 1;\n```\n\n| 日期 | 计划 |\n| --- | --- |\n| 周六 | 散步 |\n\n[链接](https://example.com)\n![远程图片](https://example.com/tracker.png)\n<script>window.markdownAttack = true</script><img src=x onerror="window.markdownAttack=true">';
  const textarea = page.getByRole('textbox', { name: '便签内容' });
  await textarea.fill(source);
  await page.getByRole('button', { name: '便签标题栏' }).dblclick();
  await expect(textarea).toHaveCount(0);
  await expect(page.getByRole('button', { name: '便签标题栏' })).toHaveAttribute(
    'aria-expanded',
    'false',
  );
  await page.getByRole('button', { name: '便签标题栏' }).dblclick();
  await expect(textarea).toHaveValue(source);
  await page.getByRole('button', { name: '便签标题栏' }).click({ button: 'right' });
  const opacity = page.getByRole('slider', { name: '便签不透明度' });
  await opacity.fill('65');
  await opacity.dispatchEvent('change');
  await expect(page.locator('.editor')).toHaveCSS('opacity', '0.65');
  await page.getByRole('button', { name: '排版编辑', exact: true }).click();
  await page.getByRole('button', { name: '便签选项' }).click();
  const preview = page.getByLabel('Markdown 预览', { exact: true });
  await expect(preview.locator('h1')).toHaveText('周末计划');
  await expect(preview.locator('strong')).toHaveText('买花');
  await expect(preview.locator('input:checked')).toBeDisabled();
  await expect(preview.locator('pre code')).toContainText('const n = 1;');
  await expect(preview.locator('table')).toContainText('周六');
  await expect(preview.locator('script, img, [onerror], [href]')).toHaveCount(0);
  expect(await page.evaluate(() => 'markdownAttack' in window)).toBe(false);
  await page.screenshot({ path: 'artifacts/markdown.png' });
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await page.getByRole('button', { name: '打开便签：# 周末计划' }).click();
  await expect(preview.locator('h1')).toHaveText('周末计划');
  await expect(page.locator('.editor')).toHaveCSS('opacity', '0.65');
  await page.getByRole('button', { name: '切换到 Markdown 源码', exact: true }).click();
  await expect(textarea).toHaveValue(source);
});

test('create, autosave, change color, search, archive, restore and reload', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByText('给小想法，留个位置')).toBeVisible();
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  const editor = page.getByRole('textbox', { name: '便签内容' });
  await editor.fill('今天的小事\n买一束花\n给家里打个电话');
  // Close immediately: pending local save must be flushed before the window disappears.
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await expect(page.getByRole('heading', { name: '今天的小事', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '打开便签：今天的小事' }).click();
  await expect(editor).toHaveValue('今天的小事\n买一束花\n给家里打个电话');
  await page.getByRole('button', { name: '便签选项' }).click();
  await page.getByRole('button', { name: '鼠尾草绿' }).click();
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await expect(page.locator('.note-card')).toHaveClass(/note-sage/);
  await page.reload();
  await expect(page.getByRole('heading', { name: '今天的小事', exact: true })).toBeVisible();
  await page.getByRole('textbox', { name: '搜索便签' }).fill('没有这个词');
  await expect(page.getByText('没有找到这张便签')).toBeVisible();
  await page.getByRole('button', { name: '清空搜索' }).click();
  await page.getByRole('button', { name: '归档 今天的小事' }).click();
  await expect(page.locator('.note-card')).toHaveCount(0);
  await page.getByRole('navigation').getByRole('button', { name: /归档/ }).click();
  await expect(page.locator('.note-card')).toHaveCount(1);
  await page.getByRole('button', { name: '恢复 今天的小事' }).click();
  await page
    .getByRole('navigation')
    .getByRole('button', { name: /我的便签/ })
    .click();
  await expect(page.locator('.note-card')).toHaveCount(1);
  expect(errors).toEqual([]);
});

test('deletion is recoverable and settings explain the credential boundary', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
  await page.getByRole('textbox', { name: '便签内容' }).fill('可以找回的便签');
  await page.getByRole('button', { name: '收起便签', exact: true }).click();
  await page.getByRole('button', { name: '删除 可以找回的便签' }).click();
  await page
    .getByRole('navigation')
    .getByRole('button', { name: /回收站/ })
    .click();
  await expect(page.locator('.note-card')).toHaveCount(1);
  await page.getByRole('button', { name: '恢复 可以找回的便签' }).click();
  await page.getByRole('button', { name: '设置与同步' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByLabel('服务地址')).toHaveValue('https://dav.jianguoyun.com/dav/');
  await expect(page.getByLabel('同步目录')).toHaveValue('QingNote');
  await expect(page.getByRole('button', { name: '测试并保存连接' })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('layout screenshot with representative text', async ({ page }) => {
  await page.goto('/');
  for (const text of [
    '今天的小事\n买一束花\n给家里打个电话\n把书读完一章',
    '灵感暂存\n让生活留一点空白。\n好想法不必立刻有答案。',
    '周末出门\n相机、电池、充电线\n一双好走的鞋',
    '工作备忘\n整理本周记录\n确认下周的安排',
  ]) {
    await page
      .getByRole('button', { name: /新建便签/ })
      .first()
      .click();
    await page.getByRole('button', { name: '切换到 Markdown 源码' }).click();
    await page.getByRole('textbox', { name: '便签内容' }).fill(text);
    await page.getByRole('button', { name: '收起便签', exact: true }).click();
  }
  await page.screenshot({ path: 'artifacts/library.png', fullPage: true });
  await page.getByRole('button', { name: '设置与同步' }).click();
  await page.screenshot({ path: 'artifacts/settings.png' });
});
