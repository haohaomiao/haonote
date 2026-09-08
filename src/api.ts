import { invoke, isTauri } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import type { Content, Note, Settings, SyncStatus } from './types';

export const native = isTauri();
// Browser preview is development-only and never bundled into the desktop app.
const preview = !native && import.meta.env.DEV ? import('./preview') : null;
export async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (native) return invoke<T>(command, args);
  if (preview) return (await preview).invokePreview(command, args) as Promise<T>;
  throw new Error('请使用haonote桌面安装版');
}
export async function on(event: string, callback: (payload: unknown) => void): Promise<() => void> {
  if (native) return listen(event, (e) => callback(e.payload));
  const handler = (e: Event) => callback((e as CustomEvent).detail);
  window.addEventListener(event, handler);
  return () => window.removeEventListener(event, handler);
}
export async function broadcast(event: string, payload?: unknown) {
  if (native) await emit(event, payload);
  else window.dispatchEvent(new CustomEvent(event, { detail: payload }));
}
export const listNotes = () => call<Note[]>('list_notes');
export const saveNote = (note: Note | null, content: Content) =>
  call<Note>('save_note', {
    noteId: note?.id ?? null,
    expected: note?.headId ?? null,
    content,
  });
export const settings = () => call<Settings>('get_settings');
export const syncStatus = () => call<SyncStatus>('sync_status');
