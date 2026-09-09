<script lang="ts">
  import { onMount } from 'svelte';
  import { X, Cloud, ShieldCheck, Download } from '@lucide/svelte';
  import { call, native, settings } from './api';
  import type { DavConfig } from './types';
  let {
    onclose,
    onchanged,
    onexport,
  }: { onclose: () => void; onchanged: () => Promise<void>; onexport: () => Promise<void> } =
    $props();
  let dialog: HTMLDialogElement;
  let url = $state('https://dav.jianguoyun.com/dav/');
  let username = $state('');
  let folder = $state('QingNote');
  let password = $state('');
  let remember = $state(true);
  let bound = $state(false);
  let connected = $state(false);
  let autostart = $state(false);
  let dataDirectory = $state('');
  let busy = $state(false);
  let message = $state('');
  let failed = $state(false);
  onMount(() => {
    dialog.showModal();
    void settings()
      .then((s) => {
        if (s.config) {
          ({ url, username, folder } = s.config);
          bound = true;
        }
        connected = s.hasPassword;
        autostart = s.autostart;
        dataDirectory = s.dataDirectory;
      })
      .catch((e) => {
        failed = true;
        message = String(e);
      });
  });
  async function connect(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    message = '';
    failed = false;
    try {
      const config: DavConfig = {
        url: url.trim(),
        username: username.trim(),
        folder: folder.trim(),
      };
      message = await call<string>('configure_sync', { config, password, remember });
      password = '';
      connected = true;
      bound = true;
      await onchanged();
      void call('sync_now').catch(() => {});
    } catch (e) {
      failed = true;
      message = String(e);
    } finally {
      busy = false;
    }
  }
  async function pause() {
    busy = true;
    try {
      message = await call<string>('forget_password');
      connected = false;
      failed = false;
      await onchanged();
    } catch (e) {
      failed = true;
      message = String(e);
    } finally {
      busy = false;
    }
  }
  async function toggleAutostart() {
    try {
      await call('set_autostart', { enabled: !autostart });
      autostart = !autostart;
    } catch (e) {
      failed = true;
      message = String(e);
    }
  }
</script>

<dialog
  bind:this={dialog!}
  class="settings-dialog"
  oncancel={(event) => {
    event.preventDefault();
    if (!busy) onclose();
  }}
>
  <header class="dialog-header">
    <div>
      <h2>设置与同步</h2>
    </div>
    <button class="icon-button" aria-label="关闭设置" onclick={onclose} disabled={busy}
      ><X size={20} /></button
    >
  </header>
  <div class="settings-scroll">
    <div class="section-title">
      <Cloud size={19} />
      <h3>让便签随你走</h3>
      <span class="tag">WebDAV</span>
    </div>
    <p class="help-text">连接坚果云或其他 WebDAV 服务。未连接时，所有便签也会保存在本机。</p>
    <form onsubmit={connect}>
      <label class="field"
        >服务地址<input
          type="url"
          bind:value={url}
          required
          disabled={busy || bound}
          spellcheck="false"
        /></label
      >
      <div class="field-pair">
        <label class="field"
          >账号<input
            bind:value={username}
            autocomplete="username"
            placeholder="坚果云注册邮箱"
            required
            disabled={busy || bound}
          /></label
        ><label class="field short-field"
          >同步目录<input
            bind:value={folder}
            pattern="[A-Za-z0-9_-]+"
            required
            disabled={busy || bound}
          /></label
        >
      </div>
      <label class="field"
        >第三方应用密码<input
          type="password"
          bind:value={password}
          autocomplete="new-password"
          placeholder={connected ? '留空则使用已连接的密码' : '在坚果云安全选项中创建'}
          required={!connected}
          disabled={busy}
        /></label
      >
      <div class="credential-help">
        <ShieldCheck size={16} /><span
          >使用应用密码，不是登录密码。勾选后保存到系统凭据库；不可用时仅在本次运行保留。</span
        >
      </div>
      <label class="checkbox-row"
        ><input type="checkbox" bind:checked={remember} disabled={busy} />记住应用密码</label
      >
      <details class="setup-guide">
        <summary>如何获取坚果云应用密码？</summary>
        <ol>
          <li>登录坚果云网页版，进入「账户信息 → 安全选项」。</li>
          <li>在「第三方应用管理」中添加应用，命名为“haonote”。</li>
          <li>复制生成的应用密码，填入上方。其他设备填写相同账号和目录。</li>
        </ol>
        <p>
          测试连接会创建同步目录，写入并删除一个临时测试文件。正文通过 HTTPS
          传输，云端文件未做端到端加密。
        </p>
      </details>
      {#if message}<div class:error={failed} class="form-message" role="status">{message}</div>{/if}
      <div class="form-actions">
        <button class="primary" type="submit" disabled={busy || !native}
          >{busy ? '正在验证连接…' : '测试并保存连接'}</button
        >{#if connected}<button type="button" class="text-button" onclick={pause} disabled={busy}
            >暂停同步</button
          >{/if}
      </div>
      {#if bound}<p class="small-note">
          已绑定此账号与目录。第一版不支持直接切换，以防止不同账号的便签混合。
        </p>{/if}
    </form>
    <div class="settings-divider"></div>
    <div class="preference-row">
      <div>
        <h3>开机启动</h3>
        <p>登录电脑后自动打开haonote。</p>
      </div>
      <button
        class="switch"
        class:enabled={autostart}
        role="switch"
        aria-checked={autostart}
        aria-label="开机启动"
        onclick={toggleAutostart}
        disabled={!native}><span></span></button
      >
    </div>
    <div class="preference-row">
      <div>
        <h3>纯文本导出</h3>
        <p>导出未删除的便签正文，方便阅读。</p>
      </div>
      <button class="secondary" onclick={onexport} disabled={!native}
        ><Download size={15} />导出 TXT</button
      >
    </div>
    <div class="data-location"><span>本地资料库</span><code>{dataDirectory}</code></div>
    <p class="small-note">
      haonote 0.1.5 · 自动同步约每 2 分钟一次。关闭便签仅收起窗口，回收站不会自动清空。
    </p>
  </div>
</dialog>
