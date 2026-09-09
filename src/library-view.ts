import { title, type Note } from './types';

export const sortOptions = {
  'updated-desc': '修改时间：最新在前',
  'updated-asc': '修改时间：最早在前',
  'created-desc': '创建时间：最新在前',
  'created-asc': '创建时间：最早在前',
};
export const periodOptions = { all: '全部时间', today: '今天', week: '近 7 天', month: '近 30 天' };
export type LibraryView = {
  layout: 'cards' | 'list';
  sort: keyof typeof sortOptions;
  period: keyof typeof periodOptions;
};
const key = 'haonote-library-view';
const defaults: LibraryView = { layout: 'cards', sort: 'updated-desc', period: 'all' };

export function loadLibraryView(): LibraryView {
  try {
    const saved = JSON.parse(localStorage.getItem(key) || '{}');
    return {
      layout: saved.layout === 'list' ? 'list' : 'cards',
      sort: Object.hasOwn(sortOptions, saved.sort) ? saved.sort : defaults.sort,
      period: Object.hasOwn(periodOptions, saved.period) ? saved.period : defaults.period,
    };
  } catch {
    return { ...defaults };
  }
}
export function saveLibraryView(view: LibraryView) {
  localStorage.setItem(key, JSON.stringify(view));
}
export function noteTime(note: Note, sort: LibraryView['sort']) {
  return sort.startsWith('created') ? note.createdAt : note.updatedAt;
}
export function selectNotes(notes: Note[], query: string, view: LibraryView, now: number) {
  const normalize = (text: string) => text.normalize('NFKC').toLocaleLowerCase();
  const words = normalize(query).trim().split(/\s+/).filter(Boolean);
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  // Calendar days in local time, including today; avoids DST/24-hour assumptions.
  start.setDate(start.getDate() - (view.period === 'week' ? 6 : view.period === 'month' ? 29 : 0));
  const date = (note: Note) => Date.parse(noteTime(note, view.sort));
  return notes
    .filter((note) => {
      const haystack = normalize(`${title(note)}\n${note.content.text}`);
      return (
        words.every((word) => haystack.includes(word)) &&
        (view.period === 'all' || (date(note) >= start.getTime() && date(note) <= now))
      );
    })
    .sort((a, b) => {
      const difference = (date(a) || 0) - (date(b) || 0);
      return (view.sort.endsWith('asc') ? difference : -difference) || a.id.localeCompare(b.id);
    });
}
