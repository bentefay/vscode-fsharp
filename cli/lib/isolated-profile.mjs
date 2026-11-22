import path from 'node:path';
import { access, lstat, mkdir, readdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { constants } from 'node:fs';

const markerName = '.fsharp-navigator-isolated.json';
const children = ['user-data', 'extensions', 'cache', 'config', 'data', 'state', 'tmp', 'runtime'];
const inside = (parent, child) => child === parent || child.startsWith(parent + path.sep);

// A portable Code launcher can override explicit directory flags from its own
// adjacent data directory. Reject that layout before invoking the trusted CLI.
export async function isolatedCode(command, { cwd = process.cwd(), env = process.env } = {}) {
  const candidates = command.includes(path.sep) ? [path.resolve(cwd, command)] :
    (env.PATH || '').split(path.delimiter).map(entry => path.resolve(cwd, entry, command));
  let executable;
  for (const candidate of candidates) {
    try { await access(candidate, constants.X_OK); executable = await realpath(candidate); break; }
    catch (error) { if (!['ENOENT', 'EACCES', 'ENOTDIR'].includes(error.code)) throw error; }
  }
  if (!executable) throw new Error(`VS Code CLI was not found or is not executable: ${command}`);
  const parent = path.dirname(executable);
  const portable = path.join(path.basename(parent) === 'bin' ? path.dirname(parent) : parent, 'data');
  try { await lstat(portable); }
  catch (error) { if (error.code === 'ENOENT') return executable; throw error; }
  throw new Error('Portable VS Code data overrides isolation flags; use a nonportable official CLI.');
}

async function checkPath(filename) {
  let current = path.parse(filename).root;
  for (const part of filename.slice(current.length).split(path.sep).filter(Boolean)) {
    current = path.join(current, part);
    try {
      const entry = await lstat(current);
      if (entry.isSymbolicLink() || !entry.isDirectory()) throw new Error(`Expected a real directory, not a link or file: ${current}`);
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
}

export async function isolatedProfile(directory, { cwd, env = process.env, create = false, requireOwned = false } = {}) {
  if (process.platform !== 'linux' || process.arch !== 'x64') throw new Error('The isolated preview installer is currently Linux x64-only.');
  if (!directory?.trim()) throw new Error('--isolated-dir requires a nonempty directory.');
  const destination = path.resolve(cwd || process.cwd(), directory);
  const defaults = [
    env.HOME && path.join(env.HOME, '.vscode'),
    env.HOME && path.join(env.HOME, '.vscode-insiders'),
    path.join(env.XDG_CONFIG_HOME || path.join(env.HOME || '/', '.config'), 'Code'),
    path.join(env.XDG_CONFIG_HOME || path.join(env.HOME || '/', '.config'), 'Code - Insiders'),
    env.VSCODE_PORTABLE, env.VSCODE_APPDATA,
  ].filter(Boolean).map(value => path.resolve(value));
  if (destination === path.parse(destination).root || defaults.some(value => inside(value, destination) || inside(destination, value))) {
    throw new Error('The isolated directory must be separate from default VS Code storage.');
  }
  await checkPath(destination);
  const directories = Object.fromEntries(children.map(name => [name, path.join(destination, name)]));
  for (const child of Object.values(directories)) await checkPath(child);
  const marker = path.join(destination, markerName);
  let owned = false;
  try {
    if (!(await lstat(marker)).isFile() || (await lstat(marker)).isSymbolicLink()) throw new Error('Invalid isolated-profile marker.');
    const value = JSON.parse(await readFile(marker, 'utf8'));
    if (value.schema !== 1 || value.kind !== 'fsharp-navigator-isolated' || value.directory !== destination) throw new Error('Isolated-profile marker does not match this directory.');
    owned = true;
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (!owned) {
    if (requireOwned) throw new Error('Uninstall requires an existing Navigator isolated-profile marker.');
    let entries = [];
    try { entries = await readdir(destination); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (entries.length) throw new Error('Choose a new or empty isolated directory; existing data is not adopted.');
  }
  if (create) {
    await mkdir(destination, { recursive: true });
    if (!owned) await writeFile(marker, JSON.stringify({ schema: 1, kind: 'fsharp-navigator-isolated', directory: destination }, null, 2) + '\n', { flag: 'wx' });
    for (const child of Object.values(directories)) {
      await mkdir(child, { recursive: true, mode: 0o700 });
      await checkPath(child);
    }
  }
  const childEnv = Object.fromEntries(Object.entries(env).filter(([key]) =>
    !key.startsWith('VSCODE_') && !key.startsWith('ELECTRON_') &&
    !['NODE_OPTIONS', 'GH_TOKEN', 'GITHUB_TOKEN', 'VSCE_PAT', 'SSH_AUTH_SOCK', 'GIT_ASKPASS', 'SSH_ASKPASS', 'BASH_ENV', 'ENV'].includes(key)));
  Object.assign(childEnv, {
    XDG_CACHE_HOME: directories.cache, XDG_CONFIG_HOME: directories.config,
    XDG_DATA_HOME: directories.data, XDG_STATE_HOME: directories.state,
    XDG_RUNTIME_DIR: directories.runtime,
    TMPDIR: directories.tmp, TMP: directories.tmp, TEMP: directories.tmp,
  });
  return { directory: destination, directories, env: childEnv,
    args: ['--force-disable-user-env', '--user-data-dir', directories['user-data'], '--extensions-dir', directories.extensions] };
}
