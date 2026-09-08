import { test, expect } from '@playwright/test';

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
