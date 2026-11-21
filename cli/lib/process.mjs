import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { mkdirSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
// Python, Rust, .NET, Node and the editor launcher inherit this workspace-backed
// default. Explicit user overrides remain available; fail if it cannot be made.
const inheritedTmp = process.env.TMPDIR && process.env.TMPDIR !== '/tmp' ? process.env.TMPDIR : undefined;
process.env.TMPDIR = path.resolve(process.env.FN_INVOCATION_CWD || root,
  process.env.FSHARP_NAVIGATOR_TMPDIR || inheritedTmp || path.join(root, '.cache', 'tmp'));
mkdirSync(process.env.TMPDIR, { recursive: true });
export async function exec(command, args = [], options = {}) {
  const { capture = false, timeout = 300000, input, measurement, ...spawnOptions } = options;
  return new Promise((resolve, reject) => {
    const processGroup = process.platform !== 'win32';
    const child = spawn(command, args, { cwd: root, stdio: capture ? [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'] : 'inherit', detached: processGroup, ...spawnOptions });
    const started = performance.now();
    const owned = new Map();
    let sampler;
    const sample = () => {
      if (!measurement || process.platform !== 'linux' || !child.pid) return;
      const table = new Map();
      for (const name of readdirSync('/proc')) {
        if (!/^\d+$/.test(name)) continue;
        try {
          const stat = readFileSync(`/proc/${name}/stat`, 'utf8').split(') ').at(-1).split(' ');
          const rss = Number(readFileSync(`/proc/${name}/status`, 'utf8').match(/^VmRSS:\s+(\d+)/m)?.[1] || 0) * 1024;
          table.set(Number(name), { parent: Number(stat[1]), started: stat[19], rss });
        } catch { /* Process may exit while sampled. */ }
      }
      if (!owned.has(child.pid) && table.has(child.pid)) owned.set(child.pid, table.get(child.pid).started);
      let found = true;
      while (found) {
        found = false;
        for (const [pid, entry] of table) {
          if (!owned.has(pid) && owned.has(entry.parent) && owned.get(entry.parent) === table.get(entry.parent)?.started) {
            owned.set(pid, entry.started); found = true;
          }
        }
      }
      const rss = [...owned].reduce((sum, [pid, stamp]) => sum + (table.get(pid)?.started === stamp ? table.get(pid).rss : 0), 0);
      measurement.helperProcessTreePeakRssBytes = Math.max(measurement.helperProcessTreePeakRssBytes || 0, rss);
      measurement.sampleCount = (measurement.sampleCount || 0) + 1;
      measurement.processesObserved = owned.size;
    };
    if (measurement) {
      Object.assign(measurement, { samplingIntervalMs: 20, memoryMetric: process.platform === 'linux' ? 'sampled sum of helper and owned descendants VmRSS; excludes CLI/editor; not PSS or cgroup memory' : 'unavailable' });
      sample(); sampler = setInterval(sample, 20);
    }
    const finishMeasurement = () => {
      clearInterval(sampler); sample();
      if (measurement) measurement.wallMs = performance.now() - started;
    };
    child.stdin?.on('error', () => {});
    if (input !== undefined) child.stdin?.end(input);
    let output = '', errors = '';
    child.stdout?.on('data', chunk => { output += chunk; });
    child.stderr?.on('data', chunk => { errors += chunk; });
    let timedOut = false;
    let killTimer;
    const kill = signal => {
      try {
        if (processGroup && child.pid) process.kill(-child.pid, signal);
        else child.kill(signal);
      } catch (error) { if (error.code !== 'ESRCH') throw error; }
    };
    const timer = setTimeout(() => {
      timedOut = true;
      kill('SIGTERM');
      killTimer = setTimeout(() => kill('SIGKILL'), 5000);
      killTimer.unref();
    }, timeout);
    child.on('error', error => { clearTimeout(timer); clearTimeout(killTimer); finishMeasurement(); reject(error); });
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      clearTimeout(killTimer);
      finishMeasurement();
      if (timedOut) kill('SIGKILL');
      if (code === 0 && !timedOut) resolve(output.trim());
      else reject(new Error(`${command} ${args.join(' ')} failed (${timedOut ? `timeout after ${timeout}ms` : signal || code})${capture ? '\n' + output + errors : ''}`));
    });
  });
}
export const npm = (...args) => exec('npm', args);
