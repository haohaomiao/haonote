export type Color = 'butter' | 'sage' | 'sky' | 'rose' | 'lilac' | 'paper';
export interface Content {
  text: string;
  title?: string;
  color: Color;
  archived: boolean;
  deleted: boolean;
}
export interface NoteView {
  collapsed: boolean;
  opacity: number;
  markdown: boolean;
  fontFamily: string;
  fontSize: number;
}
export type ViewPatch = Partial<NoteView> & { width?: number; height?: number };
export interface Revision {
  schema: number;
  id: string;
  noteId: string;
  parents: string[];
  content: Content;
  createdAt: string;
  updatedAt: string;
}
export interface Note {
  id: string;
  headId: string;
  content: Content;
  createdAt: string;
  updatedAt: string;
  conflicts: Revision[];
  pending: boolean;
}
export interface DavConfig {
  url: string;
  username: string;
  folder: string;
}
export interface Settings {
  config: DavConfig | null;
  hasPassword: boolean;
  autostart: boolean;
  dataDirectory: string;
}
export interface SyncStatus {
  running: boolean;
  lastSuccess: string | null;
  message: string;
  pending: number;
}
export const colors: { value: Color; name: string }[] = [
  { value: 'butter', name: '奶油黄' },
  { value: 'sage', name: '鼠尾草绿' },
  { value: 'sky', name: '晴空蓝' },
  { value: 'rose', name: '浅桃粉' },
  { value: 'lilac', name: '丁香紫' },
  { value: 'paper', name: '纸白' },
];
export const emptyContent = (): Content => ({
  text: '',
  color: 'butter',
  archived: false,
  deleted: false,
});
export const title = (note: Note) =>
  note.content.title?.trim() || note.content.text.trim().split('\n')[0] || '空白便签';
export const fonts = [
  { value: 'sans', name: '默认字体', css: '"Microsoft YaHei", "Noto Sans CJK SC", sans-serif' },
  { value: 'serif', name: '宋体 / 衬线', css: 'SimSun, "Noto Serif CJK SC", serif' },
  { value: 'mono', name: '等宽字体', css: 'Consolas, "Noto Sans Mono", monospace' },
];
export const shortDate = (date: string) =>
  new Date(date).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' });
