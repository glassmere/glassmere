// Runs the parity bench: each framework's runner, then the report.
//
//   node run.mjs                  everything that is installed
//   node run.mjs --skip qt        leave one out (web, qt or glassmere)
//   PARITY_PYTHON=/path/to/python node run.mjs      the Python that has PySide6
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const skipAt = args.indexOf('--skip');
const skip = skipAt >= 0 ? args[skipAt + 1].split(',') : [];
const out = join(here, 'out');
const python = process.env.PARITY_PYTHON ?? (process.platform === 'win32' ? 'python' : 'python3');
const xvfb = process.platform === 'linux' && !process.env.DISPLAY ? ['xvfb-run', ['-a']] : null;

function step(name, command, argv, options = {}) {
  if (skip.includes(name)) return console.log(`skipping ${name}`);
  console.log(`\n== ${name}`);
  const result = spawnSync(command, argv, { stdio: 'inherit', cwd: here, ...options });
  if (result.status !== 0) {
    console.error(`${name} did not finish (exit ${result.status ?? result.error?.message}); the report goes on without it.`);
  }
}

step('web', process.execPath, [join(here, 'web', 'run.mjs'), '--out', join(out, 'web')]);
step('qt', python, [join(here, 'qt', 'run.py'), '--out', join(out, 'qt')], {
  env: { ...process.env, QT_QPA_PLATFORM: process.env.QT_QPA_PLATFORM ?? 'offscreen' },
});
step('glassmere', 'cargo', [
  'run', '--release', '--quiet', '--manifest-path', join(here, 'glassmere', 'Cargo.toml'), '--', '--out', join(out, 'glassmere'),
]);
step('report', process.execPath, [join(here, 'report.mjs'), out]);
console.log(`\nOpen ${join(out, 'report.html')}`);
