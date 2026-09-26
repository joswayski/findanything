import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const build = process.argv.includes('--build');
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
if (process.platform === 'darwin') {
  run('bash', ['apps/native/macos/build.sh']);
  if (!build) run('apps/native/macos/.build/release/FindAnythingNative', []);
} else {
  const shell = process.platform === 'win32' ? 'findanything-windows' : 'findanything-linux';
  run('cargo', [build ? 'build' : 'run', '--locked', '-p', shell, ...(build ? ['--release'] : [])]);
}
