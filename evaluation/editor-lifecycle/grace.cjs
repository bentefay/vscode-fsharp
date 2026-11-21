'use strict';
const vscode = require('vscode');
const fs = require('node:fs/promises');
const path = require('node:path');
const { performance } = require('node:perf_hooks');

exports.run = async function () {
  const contract = JSON.parse(await fs.readFile(path.resolve(__dirname, '../contracts/grace46.json'), 'utf8'));
  const extension = vscode.extensions.getExtension(process.env.FSNAV_PROVIDER);
  const report = { vscode: vscode.version, alreadyActive: extension.isActive, rows: [] };
  const uri = site => vscode.Uri.file(path.join(process.env.FSNAV_GRACE_ROOT, site.path));
  async function query(doc, site, method) {
    const command = method === 'definition' ? 'vscode.executeDefinitionProvider' : 'vscode.executeReferenceProvider';
    const activeBefore = extension.isActive;
    const before = activeBefore ? extension.exports.lastResult : undefined;
    const start = performance.now();
    const values = await vscode.commands.executeCommand(command, doc.uri, new vscode.Position(site.line, site.character));
    const elapsedMs = performance.now() - start;
    const native = extension.isActive ? extension.exports.lastResult : undefined;
    const observed = native !== undefined && native !== before;
    let sessionOnMiss;
    if (!observed && extension.isActive) {
      const api = extension.exports;
      sessionOnMiss = { backendCount: api.processIds.length, queryCounts: (await api.stats()).map(s => s.queryCount) };
    }
    return { name: site.name, method, elapsedMs, activeBefore, activeAfter: extension.isActive, sessionOnMiss,
      nativeResultObserved: observed, completeness: observed ? native.completeness : null,
      reason: observed ? native.reason : null, locations: (values || []).map(value => {
        const range = value.range || value.targetSelectionRange;
        return { path: (value.uri || value.targetUri).fsPath, range: {
          start: { line: range.start.line, character: range.start.character },
          end: { line: range.end.line, character: range.end.character },
        } };
      }) };
  }
  const sites = contract.selected.filter(site => site.classification === 'source');
  for (const site of sites) {
    const doc = await vscode.workspace.openTextDocument(uri(site));
    await vscode.window.showTextDocument(doc);
    for (const method of ['definition', 'references']) {
      report.rows.push(await query(doc, site, method));
      await fs.writeFile(process.env.FSNAV_QUERY_RESULT, JSON.stringify(report));
    }
  }
  // Preserve the original 46 operations and their order before retrying the first request.
  const first = sites[0], doc = await vscode.workspace.openTextDocument(uri(first));
  await vscode.window.showTextDocument(doc);
  report.firstRequestRetry = await query(doc, first, 'definition');
  if (extension.isActive) report.stats = await extension.exports.stats();
  await fs.writeFile(process.env.FSNAV_QUERY_RESULT, JSON.stringify(report));
};
