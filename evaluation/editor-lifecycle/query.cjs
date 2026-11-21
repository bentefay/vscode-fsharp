'use strict';
const vscode = require('vscode');
const path = require('node:path');
const fs = require('node:fs/promises');
const { performance } = require('node:perf_hooks');
const expected = {
  definition: [[1, 27, 1, 28]],
  references: [[1, 27, 1, 28], [1, 32, 1, 33], [1, 36, 1, 37]],
};

exports.run = async function () {
  const extension = vscode.extensions.getExtension(process.env.FSNAV_PROVIDER);
  const result = { vscode: vscode.version, provider: extension?.id, providerVersion: extension?.packageJSON.version, alreadyActive: extension?.isActive, samples: [] };
  try {
    if (!extension) throw new Error('Requested provider is missing from disposable profile');
    const start = performance.now();
    const api = await extension.activate();
    result.activateCallMs = performance.now() - start;
    const uri = vscode.Uri.file(path.join(vscode.workspace.workspaceFolders[0].uri.fsPath, 'Lifecycle.fs'));
    const doc = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(doc);
    const query = async (stage, method, shift = 0, startTime = performance.now()) => {
      const cursor = new vscode.Position(1, 36 + shift);
      const command = method === 'definition' ? 'vscode.executeDefinitionProvider' : 'vscode.executeReferenceProvider';
      const rows = await vscode.commands.executeCommand(command, uri, cursor) || [];
      const elapsedMs = performance.now() - startTime;
      const ranges = rows.map(r => {
        const range = r.range || r.targetSelectionRange;
        return { uri: (r.uri || r.targetUri).toString(), range: [range.start.line, range.start.character, range.end.line, range.end.character] };
      }).sort((a, b) => a.range[1] - b.range[1]);
      const correctRanges = expected[method].map(([line, a, endLine, b]) => ({ uri: uri.toString(), range: [line, a + shift, endLine, b + shift] }));
      const status = ranges.length === 0 ? 'unavailable' : JSON.stringify(ranges) === JSON.stringify(correctRanges) ? 'correct' : 'wrong';
      result.samples.push({ stage, method, status, elapsedMs, ranges });
      return status;
    };
    const queryStart = performance.now();
    let status = await query('first', 'definition');
    while (status === 'unavailable' && performance.now() - queryStart < 45_000) {
      await new Promise(resolve => setTimeout(resolve, 100));
      status = await query('readinessRetry', 'definition');
    }
    if (status === 'correct') {
      result.firstUsefulProviderMs = performance.now() - queryStart;
      result.launchToFirstUsefulMs = Number(process.hrtime.bigint() - BigInt(process.env.FSNAV_LAUNCH_NS)) / 1e6;
    }
    await query('firstReferences', 'references');
    for (let i = 0; i < 8; i++) await query('warm', 'definition');
    const edit = new vscode.WorkspaceEdit();
    edit.insert(uri, new vscode.Position(1, 23), ' ');
    const beforeEdit = performance.now();
    if (!await vscode.workspace.applyEdit(edit)) throw new Error('Workspace edit was rejected');
    await query('editToAnswer', 'definition', 1, beforeEdit);
    await query('dirtyReferences', 'references', 1);
    if (extension.id.toLowerCase() === 'bentefay.fsharp-navigator') {
      const [stats] = await api.stats();
      result.engine = { helperStarts: stats.helperStarts, compilerLaunches: stats.compilerLaunches, source: api.lastResult?.source };
    }
    await vscode.commands.executeCommand('workbench.action.files.revert');
  } catch (error) {
    result.error = error.stack;
    throw error;
  } finally {
    await fs.writeFile(process.env.FSNAV_QUERY_RESULT, JSON.stringify(result));
  }
};
