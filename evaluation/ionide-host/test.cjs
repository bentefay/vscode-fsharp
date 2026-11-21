'use strict';
// Runs inside the real VS Code extension host with only the installed Ionide
// baseline and this empty measurement extension. No mock navigation provider.
const vscode = require('vscode');
const net = require('node:net');
const fs = require('node:fs');
const path = require('node:path');

function location(value) {
  if (value.uri) return {uri: value.uri.toString(), range: value.range};
  return {uri: value.targetUri.toString(), range: value.targetSelectionRange || value.targetRange};
}

async function dispatch(request) {
  const p = request.params || {};
  if (request.method === 'workspace/stats') {
    return {baseline: 'Ionide.Ionide-fsharp', version: vscode.extensions.getExtension('Ionide.Ionide-fsharp')?.packageJSON.version,
      note: 'Real VS Code provider invocation; scan/compiler counters are not exposed by Ionide.'};
  }
  if (request.method === 'workspace/open') {
    const extension = vscode.extensions.getExtension('Ionide.Ionide-fsharp');
    if (!extension) throw new Error('Ionide extension was not installed into the isolated extension directory');
    await extension.activate();
    const filename = path.join(p.root, 'Library', 'Arithmetic.fs');
    if (!fs.existsSync(filename)) throw new Error('This baseline readiness probe supports the shared fixture workspace only');
    const document = await vscode.workspace.openTextDocument(filename);
    await vscode.window.showTextDocument(document);
    const text = document.getText();
    const offset = text.indexOf('value + 1');
    if (offset < 0) throw new Error('Fixture readiness anchor missing');
    const position = document.positionAt(offset + 1);
    const started = Date.now();
    while (Date.now() - started < 90000) {
      const definitions = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', document.uri, position);
      if (definitions?.length) return {ready: true, baseline: extension.id, version: extension.packageJSON.version,
        readinessProbeMs: Date.now() - started, vscodeVersion: vscode.version};
      await new Promise(resolve => setTimeout(resolve, 200));
    }
    throw new Error('Ionide did not return the fixture readiness definition within90seconds');
  }
  if (request.method === 'document/change' || request.method === 'document/open') {
    const document = await vscode.workspace.openTextDocument(p.path);
    const edit = new vscode.WorkspaceEdit();
    edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), p.text);
    await vscode.workspace.applyEdit(edit);
    return {version: document.version};
  }
  if (request.method === 'document/close') {
    const document = await vscode.workspace.openTextDocument(p.path);
    await vscode.window.showTextDocument(document);
    await vscode.commands.executeCommand('workbench.action.files.revert');
    await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
    return {closed: true};
  }
  if (request.method.startsWith('navigation/')) {
    const document = await vscode.workspace.openTextDocument(p.path);
    await vscode.window.showTextDocument(document);
    const position = new vscode.Position(p.line, p.character);
    const operation = request.method.slice('navigation/'.length);
    if (operation === 'hover') {
      const hovers = await vscode.commands.executeCommand('vscode.executeHoverProvider', document.uri, position);
      return {completeness: 'exact', source: 'ionide-vscode-provider', hover: hovers?.length ? {
        markdown: hovers.flatMap(item => item.contents.map(content => typeof content === 'string' ? content : content.value)).join('\n'),
        range: hovers[0].range} : null};
    }
    const command = operation === 'definition' ? 'vscode.executeDefinitionProvider' : 'vscode.executeReferenceProvider';
    const values = await vscode.commands.executeCommand(command, document.uri, position) || [];
    let locations = values.map(location);
    if (operation === 'references' && !p.includeDeclaration) {
      // VS Code's command exposes no includeDeclaration flag; subtract definition
      // locations explicitly. Record that adapter behavior in the baseline report.
      const definitions = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', document.uri, position) || [];
      const declarationKeys = new Set(definitions.map(item => JSON.stringify(location(item))));
      locations = locations.filter(item => !declarationKeys.has(JSON.stringify(item)));
    }
    return {locations, completeness: 'exact', source: 'ionide-vscode-provider'};
  }
  throw new Error('Unsupported baseline method: ' + request.method);
}

exports.run = () => new Promise((resolve, reject) => {
  const socket = net.createConnection({host: '127.0.0.1', port: Number(process.env.FSHARP_BASELINE_PORT)});
  let buffer = '', chain = Promise.resolve();
  socket.on('connect', () => socket.write(JSON.stringify({ready: true}) + '\n'));
  socket.on('error', reject);
  socket.on('data', chunk => {
    buffer += chunk.toString('utf8');
    let newline;
    while ((newline = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, newline); buffer = buffer.slice(newline + 1);
      chain = chain.then(async () => {
        const request = JSON.parse(line);
        if (request.method === 'shutdown') {
          socket.end(JSON.stringify({id: request.id, result: {shutdown: true}}) + '\n');
          resolve(); return;
        }
        try {socket.write(JSON.stringify({id: request.id, result: await dispatch(request)}) + '\n');}
        catch (error) {socket.write(JSON.stringify({id: request.id, error: {message: String(error)}}) + '\n');}
      }).catch(reject);
    }
  });
});
