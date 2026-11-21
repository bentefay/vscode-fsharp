'use strict';
const vscode = require('vscode');
const fs = require('node:fs/promises');
const path = require('node:path');
const { performance } = require('node:perf_hooks');

exports.run = async function () {
  const contract = JSON.parse(await fs.readFile(path.resolve(__dirname, '../contracts/grace46.json'), 'utf8'));
  const extension = vscode.extensions.getExtension(process.env.FSNAV_PROVIDER);
  const navigator = extension.id.toLowerCase() === 'bentefay.fsharp-navigator';
  const report = { vscode: vscode.version, provider: extension.id, alreadyActive: extension.isActive, rows: [] };
  if (navigator && process.env.FSNAV_GRACE_INDEX_FIRST === '1') {
    const start = performance.now();
    await vscode.commands.executeCommand('fsharpNavigator.indexWorkspace');
    report.explicitIndexMs = performance.now() - start;
  }
  const uri = site => vscode.Uri.file(path.join(process.env.FSNAV_GRACE_ROOT, site.path));
  async function query(doc, site, method) {
    const command = method === 'definition' ? 'vscode.executeDefinitionProvider' : 'vscode.executeReferenceProvider';
    const activeBefore = extension.isActive;
    const before = navigator && activeBefore ? extension.exports.lastResult : undefined;
    const start = performance.now();
    const values = await vscode.commands.executeCommand(command, doc.uri, new vscode.Position(site.line, site.character));
    const elapsedMs = performance.now() - start;
    const native = navigator && extension.isActive ? extension.exports.lastResult : undefined;
    const observed = native !== undefined && native !== before;
    let sessionOnMiss;
    if (navigator && !observed && extension.isActive) {
      const api = extension.exports;
      sessionOnMiss = { backendCount: api.processIds.length, queryCounts: (await api.stats()).map(s => s.queryCount) };
    }
    return { name: site.name, method, elapsedMs, activeBefore, activeAfter: extension.isActive, sessionOnMiss,
      nativeResultObserved: navigator ? observed : undefined, completeness: observed ? native.completeness : undefined,
      reason: observed ? native.reason : null, locations: (values || []).map(value => {
        const range = value.range || value.targetSelectionRange;
        return { path: (value.uri || value.targetUri).fsPath, range: {
          start: { line: range.start.line, character: range.start.character },
          end: { line: range.end.line, character: range.end.character },
        } };
      }) };
  }
  const sites = contract.selected.filter(site => site.classification === 'source');
  async function replay(rows) {
    for (const site of sites) {
      const doc = await vscode.workspace.openTextDocument(uri(site));
      await vscode.window.showTextDocument(doc);
      for (const method of ['definition', 'references']) {
        rows.push(await query(doc, site, method));
        await fs.writeFile(process.env.FSNAV_QUERY_RESULT, JSON.stringify(report));
      }
    }
  }
  await replay(report.rows);
  // Preserve the original 46 operations and their order before retrying the first request.
  const first = sites[0], doc = await vscode.workspace.openTextDocument(uri(first));
  await vscode.window.showTextDocument(doc);
  report.firstRequestRetry = await query(doc, first, 'definition');
  if (navigator && extension.isActive) report.stats = await extension.exports.stats();
  if (!navigator) {
    report.readinessRetries = [];
    const deadline = performance.now() + 30_000;
    let current = report.firstRequestRetry;
    while (current.locations.length === 0 && performance.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 250));
      current = await query(doc, first, 'definition');
      report.readinessRetries.push(current);
    }
    report.subsequentRows = [];
    await replay(report.subsequentRows);
  }
  await fs.writeFile(process.env.FSNAV_QUERY_RESULT, JSON.stringify(report));
};
