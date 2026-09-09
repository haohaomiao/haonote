import { test, expect } from '@playwright/test';
import { selectNotes, type LibraryView } from '../../src/library-view';
import { emptyContent, type Note } from '../../src/types';

test('calendar filters include local midnight and sorting compares instants, not date strings', () => {
  const now = new Date(2026, 8, 8, 12).getTime();
  const at = (days: number, hour = 0) => new Date(2026, 8, 8 - days, hour).toISOString();
  const note = (id: string, updatedAt: string): Note => ({
    id,
    headId: id,
    content: emptyContent(),
    createdAt: at(60),
    updatedAt,
    conflicts: [],
    pending: false,
  });
  const notes = [
    note('today', at(0)),
    note('six', at(6)),
    note('seven', at(7)),
    note('twentynine', at(29)),
    note('thirty', at(30)),
    note('future', at(-1)),
  ];
  const view: LibraryView = { layout: 'list', sort: 'updated-desc', period: 'today' };
  expect(selectNotes(notes, '', view, now).map((n) => n.id)).toEqual(['today']);
  expect(selectNotes(notes, '', { ...view, period: 'week' }, now).map((n) => n.id)).toEqual([
    'today',
    'six',
  ]);
  expect(selectNotes(notes, '', { ...view, period: 'month' }, now).map((n) => n.id)).toEqual([
    'today',
    'six',
    'seven',
    'twentynine',
  ]);
  expect(selectNotes(notes, '', { ...view, sort: 'created-desc' }, now)).toHaveLength(0);
  const offsets = [note('a', '2026-09-08T15:00:00+08:00'), note('b', '2026-09-08T08:00:00Z')];
  expect(selectNotes(offsets, '', { ...view, period: 'all' }, now).map((n) => n.id)).toEqual([
    'b',
    'a',
  ]);
  expect(offsets.map((n) => n.id)).toEqual(['a', 'b']);
});
