<script lang="ts">
  import { onMount } from 'svelte';
  import { check, type Update } from '@tauri-apps/plugin-updater';
  import { relaunch } from '@tauri-apps/plugin-process';
  import { call, native } from './api';

  let { oninstall }: { oninstall: (install: () => Promise<void>) => Promise<void> } = $props();
  let support = $state({ enabled: false, reason: '正在读取更新配置…', version: '' });
  let update = $state<Update | null>(null);
  let phase = $state<'idle' | 'checking' | 'downloading' | 'ready' | 'installing'>('idle');
  let message = $state('');
  let received = $state(0);
  let total = $state(0);
  let installed = $state(false);
  let autoCheck = $state(true);
  let disposed = false;
  const busy = $derived(['checking', 'downloading', 'installing'].includes(phase));
  async function openReleases() {
    try {
      await call('open_releases');
    } catch {
      message = '请在浏览器打开 https://github.com/haohaomiao/haonote/releases';
    }
  }

  async function checkUpdate(automatic = false) {
    if (!support.enabled || busy || update) return;
    phase = 'checking';
    if (!automatic) message = '';
    try {
      const found = await check({ timeout: 15000 });
      if (disposed) {
        await found?.close();
        return;
      }
      update = found;
      message = found ? `发现新版本 ${found.version}` : automatic ? '' : '当前已是最新版本';
    } catch {
      if (!automatic) message = '检查失败，请稍后重试，或从发布页手动下载。当前便签不受影响。';
    } finally {
      phase = 'idle';
    }
  }

  async function download() {
    if (!update || busy) return;
    phase = 'downloading';
    message = '正在下载并验证签名，可继续编辑便签…';
    received = total = 0;
    try {
      await update.download(
        (event) => {
          if (event.event === 'Started') total = event.data.contentLength || 0;
          if (event.event === 'Progress') received += event.data.chunkLength;
        },
        { timeout: 300000 },
      );
      phase = 'ready';
      message = '下载与签名验证完成，重启后恢复已打开的便签';
    } catch {
      phase = 'idle';
      message = '下载或签名验证失败，未安装任何更新。可重试或手动下载。';
    }
  }

  async function install() {
    if (!update || phase !== 'ready') return;
    phase = 'installing';
    message = '正在保存便签并重启更新…';
    try {
      await oninstall(async () => {
        if (!installed) {
          await update!.install({ restartAfterInstall: true });
          installed = true;
        }
        // On Windows the installer exits and relaunches us; AppImage needs a restart.
        await relaunch();
      });
    } catch (error) {
      phase = 'ready';
      message = installed
        ? '新版已安装，但重启失败。请通过菜单退出后重新打开。'
        : `更新未完成：${String(error)}`;
    }
  }

  function setAutoCheck(value: boolean) {
    autoCheck = value;
    try {
      localStorage.setItem('haonote-auto-update-check', String(value));
    } catch {
      message = '检查偏好未能保存，本次设置仍有效';
    }
  }

  onMount(() => {
    if (!native) {
      support = { enabled: false, reason: '请在桌面安装版检查更新', version: '' };
      return;
    }
    try {
      autoCheck = localStorage.getItem('haonote-auto-update-check') !== 'false';
    } catch {
      /* default */
    }
    void call<typeof support>('update_support')
      .then((value) => {
        support = value;
      })
      .catch(() => {
        support.reason = '无法读取更新配置';
      });
    const startup = setTimeout(() => {
      if (autoCheck) void checkUpdate(true);
    }, 15000);
    const interval = setInterval(
      () => {
        if (autoCheck) void checkUpdate(true);
      },
      6 * 60 * 60 * 1000,
    );
    return () => {
      disposed = true;
      clearTimeout(startup);
      clearInterval(interval);
      void update?.close().catch(() => {});
    };
  });
</script>

<div class="update-panel" aria-label="应用更新">
  {#if update}<strong>新版本 {update.version}</strong>{/if}
  {#if message}<p role="status">{message}</p>{/if}
  {#if phase === 'downloading'}
    <progress aria-label="更新下载进度" value={total ? received : undefined} max={total || 1}
    ></progress>
  {/if}
  {#if phase === 'ready'}
    <button class="secondary" onclick={install}>{installed ? '重试重启' : '重启更新'}</button>
  {:else if update}
    <button class="secondary" onclick={download} disabled={busy}
      >{phase === 'downloading'
        ? '正在下载…'
        : phase === 'installing'
          ? '正在更新…'
          : '下载更新'}</button
    >
  {:else}
    <button class="text-button" onclick={() => checkUpdate()} disabled={!support.enabled || busy}
      >{phase === 'checking' ? '正在检查…' : '检查更新'}</button
    >
  {/if}
  <details>
    <summary>更新设置</summary>
    {#if support.reason}<p>{support.reason}</p>{/if}
    <label class="checkbox-row"
      ><input
        type="checkbox"
        checked={autoCheck}
        onchange={(e) => setAutoCheck(e.currentTarget.checked)}
        disabled={!support.enabled}
      />自动检查新版本</label
    >
    <p>仅检查版本，不自动安装。更新失败不影响本地便签。</p>
    <button class="text-button" onclick={openReleases}>发布与手动下载</button>
  </details>
</div>

<style>
  .update-panel {
    font-size: 12px;
    padding: 8px 12px;
  }
  p {
    line-height: 1.5;
    margin: 6px 0;
    overflow-wrap: anywhere;
  }
  progress {
    width: 100%;
  }
  details {
    margin-top: 8px;
  }
  summary {
    cursor: pointer;
  }
  .checkbox-row {
    align-items: start;
    font-size: 12px;
  }
</style>
