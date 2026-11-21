import { Command, Option } from 'clipanion';
import { access, mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { exec, root } from '../lib/process.mjs';

const absolute = file => path.resolve(process.env.FN_INVOCATION_CWD || root, file);
// Early workflow check; Rust independently enforces this semantic policy before
// accepting facts. Provenance is not authentication of an untrusted producer.
function requireMetadataContract(artifact) {
  const producer = artifact.producer;
  const components = { declarations: 2, typeNames: 1, scopeNames: 1, termNames: 1, xmlDocumentation: 2, xmlSelection: 1, callables: 1 };
  if (artifact.schemaVersion !== 2 || artifact.metadataCertificateVersion !== 1 || producer?.contract !== 'fsharp-navigator.binary-facts' || producer.algorithmId !== 'binary-facts-v2'
      || producer.runtime !== '.NET 10.0.11'
      || producer.compilerProfile?.assemblyIdentity !== 'FSharp.Compiler.Service, Version=43.12.400.0, Culture=neutral, PublicKeyToken=b03f5f7f11d50a3a'
      || producer.compilerProfile?.informationalVersion !== '43.12.400-servicing.26379.115+14fbf8d5271c98133561eb55185fdb05b286f578'
      || Object.entries(components).some(([key,value]) => producer.componentVersions?.[key] !== value)
      || Object.keys(producer.componentVersions || {}).length !== Object.keys(components).length
      || !Array.isArray(producer.implementationHashes) || producer.implementationHashes.length !== 2
      || ['exporter','fcs'].some(role => producer.implementationHashes.filter(row => row.role === role && /^[0-9a-f]{64}$/.test(row.sha256)).length !== 1)) {
    throw new Error('Metadata helper lacks the current reviewed semantic producer contract. Build the current helper and prepare again.');
  }
  return producer;
}
function preparationDiagnostics(projects) {
  const diagnostics = [...new Set(projects.flatMap(project => project.membershipDiagnostics?.length
    ? project.membershipDiagnostics : ['Unsupported environment certificate']))];
  // A profile failure can also produce many downstream membership diagnostics.
  // Keep the actual setting/toolchain rejection visible in the bounded summary.
  const priority = message => /^(Unsupported startup selector:|Noncanonical.*startup)/i.test(message) ? 0
    : /globalization|environment contract|pinned dotnet host|wrapper\/executable|resolver\/property-function/i.test(message) ? 1 : 2;
  return diagnostics.sort((left, right) => priority(left) - priority(right)).slice(0, 4).join('; ');
}
async function writeJson(file, value) {
  await mkdir(path.dirname(file), { recursive: true });
  const temp = `${file}.${process.pid}.tmp`;
  await writeFile(temp, JSON.stringify(value, null, 2) + '\n');
  await rename(temp, file);
}
async function helperPath(override) {
  const file = override ? absolute(override) : path.join(root, 'semantic/bin/Release/net10.0/FSharpNavigator.Semantic.dll');
  try { await access(file); }
  catch (error) {
    if (override || error.code !== 'ENOENT') throw error;
    await exec('dotnet', ['build', 'semantic/FSharpNavigator.Semantic.fsproj', '-c', 'Release', '--nologo', '-p:RestoreLockedMode=true']);
  }
  return file;
}
async function request(helper, method, params, timeout, metrics) {
  const measurement = metrics ? { method } : undefined;
  let output;
  try {
  output = await exec(process.env.FSHARP_NAVIGATOR_DOTNET || 'dotnet', [helper, '--once'], {
    capture: true, timeout, measurement,
    input: JSON.stringify({ id: 1, method, params }) + '\n',
    env: { ...process.env, MSBUILDDISABLENODEREUSE: '1', DOTNET_CLI_USE_MSBUILD_SERVER: '0', DOTNET_NOLOGO: '1' },
  });
  } finally {
    if (metrics) {
      metrics.phases.push(measurement);
      await writeJson(metrics.file, { schemaVersion: 1, phases: metrics.phases, note: 'Explicit preparation only. Sampling adds overhead; brief memory peaks may be missed. CLI and editor memory excluded.' });
    }
  }
  const response = output.split('\n').filter(Boolean).map(line => JSON.parse(line)).find(item => item.id === 1);
  if (!response || response.error) throw new Error(`${method}: ${response?.error?.message || 'Missing helper response'}`);
  return response.result;
}
function selection(command) {
  const params = { root: absolute(command.workspaceRoot), configuration: command.configuration };
  if (command.solution) params.solution = absolute(command.solution);
  if (command.framework) params.targetFramework = command.framework;
  return params;
}
async function exportCatalog(helper, model, selected, timeout, metrics, filename, batchSize) {
  const references = [...new Set(model.projects.flatMap(project => project.assemblyReferences || []))];
  const ownedPaths = new Set(model.projects.flatMap(project => [project.outputPath, ...(project.workspaceAssemblyReferences || [])]).filter(Boolean).map(file => path.resolve(file)));
  if (model.projects.some(project => !Array.isArray(project.workspaceAssemblyReferences))) throw new Error('Project manifest lacks workspace assembly provenance; reevaluate with the current helper before exporting libraries.');
  const external = references.filter(file => !ownedPaths.has(path.resolve(file)));
  const availableReferences = [];
  // Dependency resolution may legitimately reach a workspace binary from an
  // exported external assembly. The helper certifies only its actual closure;
  // keep existing workspace reference paths available without exporting them.
  for (const file of references) { try { await access(file); availableReferences.push(file); } catch (error) { if (error.code !== 'ENOENT') throw error; } }
  const candidates = selected.length ? selected.map(absolute) : external;
  const assemblies = candidates.filter(file => !ownedPaths.has(path.resolve(file)));
  if (assemblies.length !== candidates.length) throw new Error('Library selection includes workspace output assemblies; exclude those assemblies.');
  await mkdir(path.dirname(filename), { recursive: true });
  const shardDirectory = await mkdtemp(path.join(path.dirname(filename), path.basename(filename) + '.shards-'));
  const shards = [];
  const hashes = new Map();
  let symbolCount = 0;
  let producer;
  try {
    for (let first = 0; first < assemblies.length; first += batchSize) {
      const outputPath = path.join(shardDirectory, `${String(shards.length).padStart(4, '0')}.json`);
      const response = await request(helper, 'metadata/export', { assemblyPaths: assemblies.slice(first, first + batchSize), referencePaths: availableReferences, outputPath }, timeout, metrics);
      if (response.completeness !== 'exact' || response.factsVersion !== 2 || response.outputPath !== outputPath) throw new Error('Helper did not confirm the streamed catalog shard.');
      const currentProducer = requireMetadataContract(response);
      if (producer && JSON.stringify(producer) !== JSON.stringify(currentProducer)) throw new Error('Metadata producer changed between catalog batches.');
      producer = currentProducer;
      for (const input of response.inputHashes || []) {
        if (hashes.has(input.path) && hashes.get(input.path) !== input.sha256) throw new Error(`Library input changed between batches: ${input.path}`);
        hashes.set(input.path, input.sha256);
      }
      shards.push(path.relative(path.dirname(filename), outputPath));
      symbolCount += response.symbolCount;
    }
    if (!producer || hashes.size === 0) throw new Error('No complete external catalog was produced.');
    await writeJson(filename, { schemaVersion: 2, factsVersion: 2, metadataCertificateVersion: 1, producer, completeness: 'exact', scope: 'binary-declarations-only', catalogs: shards, inputHashes: [...hashes].map(([path, sha256]) => ({ path, sha256 })) });
    return { symbolCount, shardCount: shards.length };
  } catch (error) {
    await rm(shardDirectory, { recursive: true, force: true });
    throw error;
  }
}
async function exportEnvironment(helper, model, timeout, metrics, filename) {
  if (model.projects.some(project => !Array.isArray(project.workspaceAssemblyReferences))) throw new Error('Project manifest lacks workspace assembly provenance; reevaluate before exporting the import environment.');
  const owned = new Set(model.projects.flatMap(project => [project.outputPath, ...(project.workspaceAssemblyReferences || [])]).filter(Boolean).map(file => path.resolve(file)));
  const references = [...new Set(model.projects.flatMap(project => project.assemblyReferences || []))];
  const existing = [];
  for (const file of references) {
    try { await access(file); existing.push(file); }
    catch (error) { if (error.code !== 'ENOENT' || !owned.has(path.resolve(file))) throw error; }
  }
  const environment = await request(helper, 'metadata/environment', { referencePaths: existing, workspaceAssemblyPaths: [...owned] }, timeout, metrics);
  if (environment.completeness !== 'exact' || environment.factsVersion !== 2 || environment.typeNameInventory !== 'all-external-type-declarations' || environment.scopeNameInventory !== 'all-external-namespace-and-module-declarations' || environment.termNameInventory !== 'all-external-expression-bindings') throw new Error('Helper did not certify the complete selected import environment.');
  requireMetadataContract(environment);
  await writeJson(filename, environment);
  return environment;
}
async function exportSharedExperiment(helper, model, selected, timeout, metrics, output, memoryMb) {
  if (model.projects.some(project => !Array.isArray(project.workspaceAssemblyReferences))) throw new Error('Project manifest lacks workspace assembly provenance.');
  const owned = new Set(model.projects.flatMap(project => [project.outputPath, ...(project.workspaceAssemblyReferences || [])]).filter(Boolean).map(file => path.resolve(file)));
  const references = [...new Set(model.projects.flatMap(project => project.assemblyReferences || []))];
  const existing = [];
  for (const file of references) { try { await access(file); existing.push(file); } catch (error) { if (error.code !== 'ENOENT' || !owned.has(path.resolve(file))) throw error; } }
  const assemblies = selected.length ? selected.map(absolute) : existing.filter(file => !owned.has(path.resolve(file)));
  if (assemblies.some(file => owned.has(path.resolve(file)) || !existing.includes(file))) throw new Error('Shared-context catalog targets must be selected external references.');
  const outputDirectory = path.join(output, 'binary-generation');
  const result = await request(helper, 'metadata/prepare-experimental', { referencePaths: existing, workspaceAssemblyPaths: [...owned], assemblyPaths: assemblies, outputDirectory, maxWorkingSetMb: memoryMb, timeoutMilliseconds: timeout }, timeout + 1000, metrics);
  requireMetadataContract(result);
  if (result.kind !== 'experimental-shared-binary-preparation' || result.completeness !== 'exact' || result.importCount !== 1 || result.sourceProjectChecks !== 0 || result.outputDirectory !== outputDirectory) throw new Error('Shared-context helper did not confirm a complete binary-only generation.');
  return result;
}
async function prepareNative(kind, input, output, backend, metrics) {
  const executable = backend ? absolute(backend) : path.join(root, 'target/release/fsharp-navigator');
  try { await access(executable); } catch { throw new Error('Build the current native engine with cli/bin/run build, or supply --backend.'); }
  const measurement = metrics ? { method: `native/prepare/${kind}` } : undefined;
  let text;
  try { text = await exec(executable, ['metadata', 'prepare', kind, absolute(input), absolute(output)], { capture: true, measurement, timeout: 300000 }); }
  finally { if (metrics) { metrics.phases.push(measurement); await writeJson(metrics.file, { schemaVersion: 1, phases: metrics.phases, note: 'Explicit helper and native preparation; sampling adds overhead and can miss brief peaks. No query-time compiler.' }); } }
  return JSON.parse(text.trim());
}
export class MetadataPrepareCommand extends Command {
  static paths = [['metadata', 'prepare']];
  kind = Option.String();
  input = Option.String('--input', { required: true });
  output = Option.String('--output', { required: true });
  backend = Option.String('--backend');
  metrics = Option.String('--metrics');
  async execute() {
    if (!['catalog', 'environment'].includes(this.kind)) throw new Error('Expected catalog or environment.');
    const metrics = this.metrics ? { file: absolute(this.metrics), phases: [] } : undefined;
    console.log(JSON.stringify(await prepareNative(this.kind, this.input, this.output, this.backend, metrics), null, 2));
  }
}
class ProjectOptions extends Command {
  workspaceRoot = Option.String('--root', { required: true });
  solution = Option.String('--solution');
  configuration = Option.String('--configuration', 'Debug');
  framework = Option.String('--framework');
  helper = Option.String('--helper');
  timeout = Option.String('--timeout-ms', '300000');
  metrics = Option.String('--metrics');
  batchSize = Option.String('--library-batch-size', '1');
  async helperContext() {
    const timeout = Number(this.timeout);
    if (!Number.isSafeInteger(Number(this.batchSize)) || Number(this.batchSize) < 1 || Number(this.batchSize) > 128) throw new Error('--library-batch-size must be an integer from 1 to 128.');
    if (!Number.isSafeInteger(timeout) || timeout < 1) throw new Error('--timeout-ms must be a positive integer.');
    return { helper: await helperPath(this.helper), params: selection(this), timeout, metrics: this.metrics ? { file: absolute(this.metrics), phases: [] } : undefined };
  }
}
export class EvaluateCommand extends ProjectOptions {
  static paths = [['projects', 'evaluate']];
  output = Option.String('--output', { required: true });
  async execute() {
    const { helper, params, timeout, metrics } = await this.helperContext();
    const model = await request(helper, 'workspace/evaluate', params, timeout, metrics);
    await writeJson(absolute(this.output), model);
    console.log(`Evaluated ${model.projects.length} projects: ${absolute(this.output)}`);
  }
}
export class LibrariesExportCommand extends ProjectOptions {
  static paths = [['libraries', 'export']];
  output = Option.String('--output', { required: true });
  assemblies = Option.Array('--assembly', []);
  model = Option.String('--model');
  async execute() {
    const { helper, params, timeout, metrics } = await this.helperContext();
    const model = this.model ? JSON.parse(await readFile(absolute(this.model), 'utf8')) : await request(helper, 'workspace/evaluate', params, timeout, metrics);
    const catalog = await exportCatalog(helper, model, this.assemblies, timeout, metrics, absolute(this.output), Number(this.batchSize));
    console.log(`Exported ${catalog.symbolCount} declarations: ${absolute(this.output)}`);
  }
}
export class EnvironmentExportCommand extends ProjectOptions {
  static paths = [['libraries', 'environment']];
  output = Option.String('--output', { required: true });
  model = Option.String('--model');
  async execute() {
    const { helper, params, timeout, metrics } = await this.helperContext();
    const model = this.model ? JSON.parse(await readFile(absolute(this.model), 'utf8')) : await request(helper, 'workspace/evaluate', params, timeout, metrics);
    const environment = await exportEnvironment(helper, model, timeout, metrics, absolute(this.output));
    console.log(`Certified ${environment.visitedEntities} external declarations: ${absolute(this.output)}`);
  }
}
export class PrepareCommand extends ProjectOptions {
  static paths = [['prepare']];
  output = Option.String('--out', { required: true });
  semantic = Option.Boolean('--semantic-snapshot', false);
  assemblies = Option.Array('--assembly', []);
  nativeIndex = Option.Boolean('--native-index', false);
  backend = Option.String('--backend');
  sharedContext = Option.Boolean('--experimental-shared-context', false);
  sharedMemoryMb = Option.String('--shared-context-max-mb', '512');
  async execute() {
    if (this.backend && !this.nativeIndex) throw new Error('--backend requires --native-index.');
    if (process.env.FN_DEVENV === '1') console.error('The committed Nix toolchain has no validated reusable-preparation profile. The helper checks the actual SDK, loader and locale inputs; unsupported contexts cannot generate editor settings. See nix/README.md.');
    const output = absolute(this.output);
    if (this.sharedContext && (!Number.isSafeInteger(Number(this.sharedMemoryMb)) || Number(this.sharedMemoryMb) < 32 || Number(this.sharedMemoryMb) > 4096)) throw new Error('--shared-context-max-mb must be an integer from 32 to 4096.');
    if (this.nativeIndex || this.sharedContext) {
      // Reserve a fresh generation before writing even its project model.
      // A failed retry must not change files referenced by older settings.
      await mkdir(path.dirname(output), { recursive: true });
      try { await mkdir(output); }
      catch (error) { if (error.code === 'EEXIST') throw new Error('Experimental/native-index preparation requires a new --out directory; the existing generation was left unchanged.'); throw error; }
    }
    const { helper, params, timeout, metrics } = await this.helperContext();
    const model = await request(helper, 'workspace/evaluate', params, timeout, metrics);
    const modelFile = path.join(output, 'projects.json');
    await writeJson(modelFile, model);
    const unsupported = model.projects.filter(project => project.membershipReusable !== true || project.environmentCertificate?.version !== 4 || project.environmentCertificate?.contract !== 'dotnet-msbuild-observed-v4');
    if (unsupported.length) throw new Error(`Project preparation cannot certify reusable source order: ${preparationDiagnostics(unsupported)}. No editor settings were generated. Review all diagnostics in ${modelFile} for ${unsupported.map(project => project.path).join(', ')}.`);
    let catalogFile = path.join(output, 'libraries.json');
    let environmentFile = path.join(output, 'builtins.json');
    let catalog;
    if (this.sharedContext) {
      const shared = await exportSharedExperiment(helper, model, this.assemblies, timeout, metrics, output, Number(this.sharedMemoryMb));
      catalogFile = shared.catalogPath; environmentFile = shared.environmentPath;
      catalog = { symbolCount: shared.catalogSymbols, shardCount: shared.catalogShards };
    } else {
      catalog = await exportCatalog(helper, model, this.assemblies, timeout, metrics, catalogFile, Number(this.batchSize));
      await exportEnvironment(helper, model, timeout, metrics, environmentFile);
    }
    const settings = { 'fsharpNavigator.mode': 'indexed', 'fsharpNavigator.evaluatedModelPath': modelFile, 'fsharpNavigator.libraryCatalogPath': catalogFile, 'fsharpNavigator.builtinEnvironmentPath': environmentFile };
    if (this.nativeIndex) {
      const preparedCatalog = path.join(output, 'libraries.fnmeta');
      const preparedEnvironment = path.join(output, 'builtins.fnmeta');
      await prepareNative('catalog', catalogFile, preparedCatalog, this.backend, metrics);
      await prepareNative('environment', environmentFile, preparedEnvironment, this.backend, metrics);
      settings['fsharpNavigator.libraryCatalogPath'] = preparedCatalog;
      settings['fsharpNavigator.builtinEnvironmentPath'] = preparedEnvironment;
    }
    if (this.semantic) {
      console.error('Explicit semantic snapshot export checks source context; it may take substantial time and memory.');
      const snapshot = await request(helper, 'semantic/export', params, timeout, metrics);
      const snapshotFile = path.join(output, 'semantic.json');
      await writeJson(snapshotFile, snapshot);
      if (snapshot.projects?.some(project => project.completeness !== 'exact' || project.membershipReusable !== true || project.environmentCertificate?.version !== 4)) throw new Error(`Semantic export contains incomplete projects; review ${snapshotFile}. No semantic setting was generated.`);
      settings['fsharpNavigator.semanticSnapshotPath'] = snapshotFile;
    }
    await writeJson(path.join(output, 'settings.json'), settings);
    console.log(JSON.stringify({ output, projects: model.projects.length, declarations: catalog.symbolCount, settings, preparation: 'explicit one-shot helper; indexed navigation starts no compiler' }, null, 2));
  }
}
