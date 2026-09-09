import { broadcast, call, on } from './api';

// Listen before requesting saves; clean up even if registration or broadcasting fails.
export async function flushEditors(freeze = false): Promise<void> {
  const ids = new Set(await call<string[]>('editor_ids'));
  if (!ids.size) return;
  const token = crypto.randomUUID();
  let off = () => {};
  let settled = false;
  let timer: ReturnType<typeof setTimeout>;
  try {
    await new Promise<void>((resolve, reject) => {
      const finish = (error?: Error) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        error ? reject(error) : resolve();
      };
      timer = setTimeout(
        () => finish(new Error('便签未能确认保存，操作已取消，请检查后重试')),
        8000,
      );
      void on('editor-flushed', (payload) => {
        const ack = payload as { token: string; noteId: string; ok: boolean };
        if (ack.token !== token || !ids.has(ack.noteId)) return;
        if (!ack.ok) return finish(new Error('有便签尚未保存或正在输入，请完成编辑后重试'));
        ids.delete(ack.noteId);
        if (!ids.size) finish();
      })
        .then(async (unlisten) => {
          off = unlisten;
          if (settled) return off();
          await broadcast(freeze ? 'prepare-editors-update' : 'flush-editors', token);
        })
        .catch((error) => finish(error));
    });
  } finally {
    settled = true;
    clearTimeout(timer!);
    off();
  }
}
