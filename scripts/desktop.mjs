// Prefer the user's Rust. This workspace also supports the isolated toolchain used to build it.
import { spawn, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { delimiter, join } from 'node:path';

const env = { ...process.env };
if (spawnSync('cargo', ['--version'], { stdio: 'ignore' }).status !== 0) {
  const tools = fileURLToPath(new URL('../../.qingnote-tools/', import.meta.url));
  if (existsSync(join(tools, 'cargo', 'bin', 'cargo'))) {
    env.CARGO_HOME = join(tools, 'cargo');
    env.RUSTUP_HOME = join(tools, 'rustup');
    env.PATH = join(tools, 'cargo', 'bin') + delimiter + (env.PATH || '');
  }
}
const child = spawn(
  'npm',
  ['run', 'tauri', '--', 'dev', '--config', 'src-tauri/tauri.dev.conf.json'],
  {
    cwd: fileURLToPath(new URL('../', import.meta.url)),
    env,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  },
);
child.on('error', (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
child.on('exit', (code) => {
  process.exitCode = code ?? 1;
});
