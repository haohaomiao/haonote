import { test, expect, type Page } from '@playwright/test';

async function mockDesktop(
  page: Page,
  options: {
    saveFails?: boolean;
    downloadFails?: boolean;
    noUpdate?: boolean;
    noAck?: boolean;
  } = {},
) {
  await page.addInitScript((options) => {
    const callbacks = new Map<number, (value: unknown) => void>();
    const listeners = new Map<number, string>();
    const trace: string[] = [];
    let next = 0;
    Object.assign(window, {
      isTauri: true,
      updateTrace: trace,
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        transformCallback: (fn: (value: unknown) => void) => {
          callbacks.set(++next, fn);
          return next;
        },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: async (command: string, args: Record<string, any> = {}) => {
          trace.push(
            command === 'plugin:event|emit'
              ? args.event
              : command === 'prepare_update'
                ? `prepare:${args.preparing}`
                : command,
          );
          if (command === 'list_notes') return [];
          if (command === 'sync_status' || command === 'sync_now')
            return { running: false, pending: 0, message: '', lastSuccess: null };
          if (command === 'update_support') return { version: '0.1.5', enabled: true, reason: '' };
          if (command === 'plugin:updater|check')
            return options.noUpdate
              ? null
              : { rid: 1, currentVersion: '0.1.5', version: '0.1.6', rawJson: {} };
          if (command === 'plugin:updater|download') {
            if (options.downloadFails) throw new Error('signature mismatch');
            return 2;
          }
          if (command === 'editor_ids') return ['a', 'b'];
          if (command === 'plugin:event|listen') {
            listeners.set(args.handler, args.event);
            return args.handler;
          }
          if (command === 'plugin:event|unlisten') {
            listeners.delete(args.eventId);
            return;
          }
          if (command === 'plugin:event|emit' && args.event === 'prepare-editors-update') {
            if (options.noAck) return;
            for (const noteId of ['a', 'b']) {
              for (const [id, name] of listeners) {
                if (name === 'editor-flushed')
                  callbacks.get(id)?.({
                    payload: {
                      token: args.payload,
                      noteId,
                      ok: !(options.saveFails && noteId === 'b'),
                    },
                  });
              }
            }
          }
        },
      },
    });
  }, options);
  await page.goto('/');
  await expect(page.getByRole('button', { name: '检查更新', exact: true })).toBeEnabled();
}

test('update download is separate from saving and install waits for all editors', async ({
  page,
}) => {
  await mockDesktop(page);
  await page.getByRole('button', { name: '检查更新', exact: true }).click();
  await page.getByRole('button', { name: '下载更新', exact: true }).click();
  await expect(page.getByRole('button', { name: '重启更新', exact: true })).toBeVisible();
  const trace = () => page.evaluate(() => (window as any).updateTrace as string[]);
  expect(await trace()).not.toContain('prepare:true');
  await page.getByRole('button', { name: '重启更新', exact: true }).click();
  await expect.poll(trace).toContain('plugin:process|restart');
  const commands = await trace();
  expect(commands.indexOf('prepare-editors-update')).toBeLessThan(
    commands.indexOf('plugin:updater|install'),
  );
  expect(commands.indexOf('plugin:updater|install')).toBeLessThan(
    commands.indexOf('plugin:process|restart'),
  );
});

test('failed editor save aborts installation and releases editors', async ({ page }) => {
  await mockDesktop(page, { saveFails: true });
  await page.getByRole('button', { name: '检查更新', exact: true }).click();
  await page.getByRole('button', { name: '下载更新', exact: true }).click();
  await page.getByRole('button', { name: '重启更新', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('尚未保存');
  const commands: string[] = await page.evaluate(() => (window as any).updateTrace);
  expect(commands).not.toContain('plugin:updater|install');
  expect(commands).toContain('prepare:false');
  expect(commands).toContain('resume-editors');
  await expect(page.locator('.library-shell')).not.toHaveAttribute('inert');
});

test('download or signature failure never reaches installation', async ({ page }) => {
  await mockDesktop(page, { downloadFails: true });
  await page.getByRole('button', { name: '检查更新', exact: true }).click();
  await page.getByRole('button', { name: '下载更新', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('下载或签名验证失败');
  await expect(page.getByRole('button', { name: '重启更新', exact: true })).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).updateTrace)).not.toContain(
    'plugin:updater|install',
  );
});

test('no new version is reported without showing download', async ({ page }) => {
  await mockDesktop(page, { noUpdate: true });
  await page.getByRole('button', { name: '检查更新', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('当前已是最新版本');
  await expect(page.getByRole('button', { name: '下载更新', exact: true })).toHaveCount(0);
});

test('an unresponsive editor times out without installing and removes its listener', async ({
  page,
}) => {
  await mockDesktop(page, { noAck: true });
  await page.getByRole('button', { name: '检查更新', exact: true }).click();
  await page.getByRole('button', { name: '下载更新', exact: true }).click();
  await page.getByRole('button', { name: '重启更新', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('未能确认保存', { timeout: 12000 });
  const commands: string[] = await page.evaluate(() => (window as any).updateTrace);
  expect(commands).not.toContain('plugin:updater|install');
  expect(commands).toContain('plugin:event|unlisten');
  expect(commands).toContain('resume-editors');
});

test('a real editor freezes after flushing and can resume without losing text', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: '写下第一张便签' }).click();
  const rich = page.locator('.tiptap');
  await rich.fill('更新前的输入');
  await page.evaluate(() =>
    window.dispatchEvent(new CustomEvent('prepare-editors-update', { detail: 'test-token' })),
  );
  await expect(page.getByRole('region', { name: '便签编辑器' })).toHaveAttribute('inert');
  await expect(rich).toHaveAttribute('contenteditable', 'false');
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('resume-editors')));
  await expect(rich).toHaveAttribute('contenteditable', 'true');
  await expect(rich).toContainText('更新前的输入');
  await rich.fill('恢复编辑后');
  await expect(page.getByText('已保存在本机', { exact: true })).toBeVisible();
});
