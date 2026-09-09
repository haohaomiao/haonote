import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { prepareRelease } from '../scripts/release-assets.mjs';

async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), 'haonote-release-test-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const source = join(directory, 'source');
  const nested = join(source, 'target', 'release', 'bundle');
  await mkdir(nested, { recursive: true });
  const names = [
    'haonote_0.1.5_x64-setup.exe',
    'haonote_0.1.5_amd64.AppImage',
    'haonote_0.1.5_amd64.deb',
  ];
  for (const name of names) {
    await writeFile(join(nested, name), 'test installer');
  }
  return { source, nested, destination: join(directory, 'output'), names };
}

test('unsigned release collects three installers and correct checksums from nested target paths', async (t) => {
  const { source, destination, names } = await fixture(t);
  await prepareRelease(source, destination, '0.1.5');
  assert.deepEqual((await readdir(destination)).sort(), [...names, 'SHA256SUMS'].sort());
  const hash = createHash('sha256').update('test installer').digest('hex');
  assert.equal(
    await readFile(join(destination, 'SHA256SUMS'), 'utf8'),
    names.map((name) => `${hash}  ${name}\n`).join(''),
  );
});

test('release refuses missing installers', async (t) => {
  const { source, nested, destination, names } = await fixture(t);
  await rm(join(nested, names[0]));
  await assert.rejects(prepareRelease(source, destination, '0.1.5'), /Expected exactly one.*exe/);
});

test('release refuses duplicate filenames and incorrect version', async (t) => {
  const { source, destination, names } = await fixture(t);
  await writeFile(join(source, names[0]), 'duplicate');
  await assert.rejects(prepareRelease(source, destination, '0.1.5'), /found 2/);
  await assert.rejects(prepareRelease(source, destination, '0.1.6'), /found 0/);
});

test('updater leftovers are not collected and an existing output directory is rejected', async (t) => {
  const { source, nested, destination, names } = await fixture(t);
  await writeFile(join(nested, `${names[0]}.sig`), 'old signature');
  await writeFile(join(nested, 'latest.json'), '{}');
  await prepareRelease(source, destination, '0.1.5');
  assert.deepEqual((await readdir(destination)).sort(), [...names, 'SHA256SUMS'].sort());
  await assert.rejects(prepareRelease(source, destination, '0.1.5'), /EEXIST/);
});
