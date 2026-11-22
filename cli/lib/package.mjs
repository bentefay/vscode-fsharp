import { mkdir, readFile, writeFile, copyFile, chmod, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { exec, root } from './process.mjs';

export const manifest = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
export const vsixPath = path.join(root, 'artifacts', `${manifest.name}-${manifest.version}-linux-x64.vsix`);
export async function inspectPackage(filename, expectedNativeHash) {
  const entries = (await exec('unzip', ['-Z1', filename], { capture: true })).split('\n');
  const bundled = JSON.parse(await exec('unzip', ['-p', filename, 'extension/package.json'], { capture: true }));
  if (bundled.publisher !== manifest.publisher || bundled.name !== manifest.name) throw new Error('VSIX identity mismatch.');
  for (const required of ['extension/THIRD_PARTY_NOTICES.md', 'extension/extension/dist/extension.js', 'extension/extension/dist/backend.js', 'extension/extension/dist/overlays.js', 'extension/extension/dist/trace.js', 'extension/extension/dist/virtualDocuments.js', 'extension/bin/linux-x64/fsharp-navigator']) {
    if (!entries.includes(required)) throw new Error(`Missing ${required}`);
  }
  const notices = await exec('unzip', ['-p', filename, 'extension/THIRD_PARTY_NOTICES.md'], { capture: true });
  const grammarLicense = (await readFile(path.join(root, 'vendor/tree-sitter-fsharp/LICENSE'), 'utf8')).trim();
  if (!notices.includes('## tree-sitter-fsharp 0.3.13') || !notices.includes(grammarLicense)) throw new Error('Vendored F# grammar license is missing from distribution notices.');
  const allowed = /^(?:\[Content_Types\]\.xml|extension\.vsixmanifest|extension\/(?:package\.json|readme\.md|LICENSE\.txt|THIRD_PARTY_NOTICES\.md|bin\/linux-x64\/fsharp-navigator|extension\/language-configuration\.json|extension\/resources\/fsharp\.svg|extension\/dist\/[^/]+\.js))$/;
  const forbidden = entries.filter(p => !p.endsWith('/') && !allowed.test(p));
  if (forbidden.length) throw new Error(`Unexpected VSIX paths: ${forbidden.join(', ')}`);
  const native = JSON.parse(await exec('python3', ['-c', 'import hashlib,json,sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); p="extension/bin/linux-x64/fsharp-navigator"; print(json.dumps({"sha256":hashlib.sha256(z.read(p)).hexdigest(),"executable":bool((z.getinfo(p).external_attr>>16)&0o111)}))', filename], {capture:true}));
  if (!native.executable) throw new Error('Bundled native executable lost its executable permission.');
  if (expectedNativeHash && native.sha256 !== expectedNativeHash) throw new Error('Bundled native binary differs from the reviewed build.');
  return bundled;
}
export function packageOutputPath(output) {
  return output === undefined ? vsixPath : path.resolve(process.env.FN_INVOCATION_CWD || root, output);
}
export async function buildPackage(output) {
  const destination = packageOutputPath(output);
  if (process.platform !== 'linux' || process.arch !== 'x64') throw new Error('This prototype package target is linux-x64 only.');
  await exec('cargo', ['build', '--release', '--locked']);
  await exec('npm', ['run', 'compile']);
  await exec('python3', ['scripts/third-party-notices.py']);
  const dir = path.join(root, 'bin', 'linux-x64');
  await mkdir(dir, { recursive: true });
  const binary = path.join(dir, 'fsharp-navigator');
  await copyFile(path.join(root, 'target/release/fsharp-navigator'), binary);
  await chmod(binary, 0o755);
  const nativeHash = createHash('sha256').update(await readFile(binary)).digest('hex');
  await mkdir(path.dirname(destination), { recursive: true });
  await exec('node', ['node_modules/@vscode/vsce/vsce', 'package', '--no-dependencies', '--allow-missing-repository', '--target', 'linux-x64', '--out', destination]);
  const sourceEpoch = process.env.SOURCE_DATE_EPOCH || (await exec('git', ['show', '-s', '--format=%ct', 'HEAD'], { capture: true })).trim();
  if (!/^\d+$/.test(sourceEpoch)) throw new Error('SOURCE_DATE_EPOCH must be a nonnegative integer.');
  await exec('python3', ['scripts/normalize-vsix.py', destination, '--epoch', sourceEpoch]);
  await inspectPackage(destination, nativeHash);
  const hash = createHash('sha256').update(await readFile(destination)).digest('hex');
  await writeFile(destination + '.sha256', `${hash}  ${path.basename(destination)}\n`);
  await writeFile(destination + '.build.json', JSON.stringify({ sourceDateEpoch: Number(sourceEpoch), vsixSha256: hash, nativeSha256: nativeHash, scope: 'Same source and pinned build toolchain; not a cross-host Rust reproducibility claim' }, null, 2) + '\n');
  console.log(`${destination}\nSHA-256 ${hash}\nNative SHA-256 ${nativeHash}\nBytes ${(await stat(destination)).size}`);
  return destination;
}
