import { readdir, readFile, mkdir, copyFile, writeFile } from 'node:fs/promises';
import { join, basename, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';

async function filesIn(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const groups = await Promise.all(
    entries.map((entry) => {
      const path = join(directory, entry.name);
      return entry.isDirectory() ? filesIn(path) : entry.isFile() ? [path] : [];
    }),
  );
  return groups.flat();
}

export async function prepareRelease(source, destination, version) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Expected a numeric release version');
  const files = await filesIn(source);
  const required = [
    `haonote_${version}_x64-setup.exe`,
    `haonote_${version}_amd64.AppImage`,
    `haonote_${version}_amd64.deb`,
  ];
  const contents = new Map();
  for (const name of required) {
    const matches = files.filter((path) => basename(path) === name);
    if (matches.length !== 1)
      throw new Error(`Expected exactly one ${name}, found ${matches.length}`);
    const bytes = await readFile(matches[0]);
    if (!bytes.length) throw new Error(`Empty release asset: ${name}`);
    contents.set(name, { path: matches[0], bytes });
  }
  // Require a fresh output directory so stale updater files cannot be published by '*'.
  await mkdir(destination);
  for (const [name, { path }] of contents) await copyFile(path, join(destination, name));
  const sums = [...contents].map(
    ([name, { bytes }]) => `${createHash('sha256').update(bytes).digest('hex')}  ${name}`,
  );
  await writeFile(join(destination, 'SHA256SUMS'), sums.join('\n') + '\n');
  return required;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const { version } = JSON.parse(await readFile('package.json', 'utf8'));
  await prepareRelease('release-artifacts', 'release-assets', version);
}
