'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const Module = require('node:module');
const { fileURLToPath, pathToFileURL } = require('node:url');

// This is explicitly an API-boundary double. It executes the shipped adapter
// and real subprocess transport, but neither starts nor measures VS Code.
class Uri {
  constructor(value) { this.value = new URL(value); }
  static parse(value) { return new Uri(value); }
  static file(value) { return new Uri(pathToFileURL(value).toString()); }
  get scheme() { return this.value.protocol.slice(0, -1); }
  get authority() { return this.value.host; }
  get path() { return this.value.pathname; }
  get query() { return this.value.search.slice(1); }
  get fragment() { return this.value.hash.slice(1); }
  get fsPath() { return fileURLToPath(this.value); }
  toString() { return this.value.toString(); }
}
class EventEmitter {
  listeners = new Set();
  event = listener => { this.listeners.add(listener); return { dispose: () => this.listeners.delete(listener) }; };
  fire(value) { for (const listener of this.listeners) listener(value); }
  dispose() { this.listeners.clear(); }
}
class Position { constructor(line, character) { this.line = line; this.character = character; } isEqual(other) { return this.line === other.line && this.character === other.character; } }
class Range { constructor(sl, sc, el, ec) { this.start = new Position(sl, sc); this.end = new Position(el, ec); } }
class Location { constructor(uri, range) { this.uri = uri; this.range = range; } }
class CancellationError extends Error {}
class CancellationTokenSource {
  constructor() {
    this.changed = new EventEmitter();
    this.token = { isCancellationRequested: false, onCancellationRequested: this.changed.event };
  }
  cancel() { if (!this.token.isCancellationRequested) { this.token.isCancellationRequested = true; this.changed.fire(); } }
  dispose() { this.changed.dispose(); }
}
const noop = () => ({ dispose() {} });
const uncancelled = { isCancellationRequested: false, onCancellationRequested: noop };

exports.createEditor = async function (directory, binary, configuration = {}) {
  const folders = (configuration.workspaceDirectories || [directory]).map((value, index) => ({ uri: Uri.file(value), index, name: path.basename(value) }));
  const documents = [];
  const providers = {};
  const events = Object.fromEntries(['open', 'change', 'save', 'close', 'configuration', 'folders', 'output', 'selection', 'activeEditor'].map(name => [name, new EventEmitter()]));
  const output = [];
  const warnings = [];
  const warningCalls = [];
  const resources = { outputChannels: 0, statusBarItems: 0, fileWatchers: 0 };
  const commands = new Map();
  const openedUris = [];
  const candidateViews = [];
  const statusItems = [];
  const quickPicks = [];
  const makeStatus = () => ({ visible: false, show() { this.visible = true; }, hide() { this.visible = false; }, dispose() { this.disposed = true; this.visible = false; } });
  const status = makeStatus();
  const context = { extensionPath: path.resolve(__dirname, '../../..'), subscriptions: [], storageUri: Uri.file(path.join(directory, '.storage')), globalStorageUri: Uri.file(path.join(directory, '.global')) };
  const settings = { mode: 'indexed', backendPath: binary, ...configuration };
  const boundary = {
    Uri, Range, Position, Location, EventEmitter, CancellationError, CancellationTokenSource,
    StatusBarAlignment: { Right: 2 }, TreeItemCollapsibleState: { Collapsed: 1, None: 0 },
    ConfigurationTarget: { Workspace: 2 },
    TreeItem: class { constructor(label, collapsibleState) { this.label = label; this.collapsibleState = collapsibleState; } },
    ThemeIcon: class { constructor(id) { this.id = id; } },
    MarkdownString: class { constructor(value) { this.value = value; } },
    Hover: class { constructor(contents, range) { this.contents = contents; this.range = range; } },
    workspace: {
      isTrusted: true, workspaceFolders: folders, textDocuments: documents,
      getWorkspaceFolder(uri) { return uri.scheme === 'file' ? folders.filter(folder => uri.fsPath === folder.uri.fsPath || uri.fsPath.startsWith(folder.uri.fsPath + path.sep)).sort((a, b) => b.uri.fsPath.length - a.uri.fsPath.length)[0] : undefined; },
      getConfiguration() { return { get: (name, fallback) => settings[name] ?? fallback, update: async (name, value) => { settings[name] = value; events.configuration.fire({ affectsConfiguration: () => true }); } }; },
      onDidOpenTextDocument: events.open.event, onDidChangeTextDocument: events.change.event,
      onDidSaveTextDocument: events.save.event, onDidCloseTextDocument: events.close.event,
      onDidChangeConfiguration: events.configuration.event, onDidChangeWorkspaceFolders: events.folders.event,
      createFileSystemWatcher() { resources.fileWatchers++; return { onDidCreate: noop, onDidChange: noop, onDidDelete: noop, dispose() {} }; },
      registerTextDocumentContentProvider: noop,
    },
    ProgressLocation: { Notification: 15 },
    window: {
      activeTextEditor: undefined,
      async withProgress(options, task) { const token = configuration.progressToken || uncancelled; return configuration.progress ? configuration.progress(task, token) : task({ report() {} }, token); },
      onDidChangeTextEditorSelection: events.selection.event,
      onDidChangeActiveTextEditor: events.activeEditor.event,
      createOutputChannel() { resources.outputChannels++; return { append: text => { output.push(text); events.output.fire(text); }, appendLine: text => { output.push(text); events.output.fire(text); }, show() {}, dispose() {} }; },
      createStatusBarItem() { const item = resources.statusBarItems++ === 0 ? status : makeStatus(); statusItems.push(item); return item; }, registerTreeDataProvider: noop,
      createQuickPick() {
        const accept = new EventEmitter(), hide = new EventEmitter();
        const picker = { items: [], selectedItems: [], visible: false, busy: false,
          onDidAccept: accept.event, onDidHide: hide.event,
          show() { this.visible = true; },
          hide() { if (this.visible) { this.visible = false; hide.fire(); } },
          dispose() { this.disposed = true; this.visible = false; accept.dispose(); hide.dispose(); },
          accept(item = this.items[0]) { this.selectedItems = [item]; accept.fire(); },
        };
        quickPicks.push(picker); return picker;
      },
      createTreeView(id, options) {
        const view = { id, options, visible: false, reveals: [], async reveal(item, settings) { if (!options.treeDataProvider.getParent) throw new Error('getParent required'); this.visible = true; this.reveals.push({ item, settings }); if (configuration.reveal) await configuration.reveal(item, settings); }, dispose() {} };
        candidateViews.push(view); return view;
      },
      showWarningMessage: async (text, ...actions) => { warnings.push(text); const call = { text, actions }; warningCalls.push(call); return configuration.warning ? configuration.warning(call) : undefined; }, showQuickPick: async () => undefined,
    },
    languages: {
      registerDefinitionProvider(selector, provider) { providers.definition = provider; return noop(); },
      registerReferenceProvider(selector, provider) { providers.references = provider; return noop(); },
      registerHoverProvider(selector, provider) { providers.hover = provider; return noop(); },
    },
    commands: {
      registerCommand(name, fn) { commands.set(name, fn); return { dispose() { commands.delete(name); } }; },
      async executeCommand(name, ...args) { if (name === 'setContext' || name === 'fsharpNavigator.possibleReferences.focus') return; if (name === 'vscode.open') { openedUris.push(args[0]); return; } const command = commands.get(name); if (!command) throw new Error(`Unknown command ${name}`); return command(...args); },
    },
  };
  const load = Module._load;
  for (const file of ['extension', 'backend', 'overlays', 'virtualDocuments', 'trace', 'possibleReferences']) delete require.cache[require.resolve(`../../dist/${file}.js`)];
  Module._load = function (name, ...args) { return name === 'vscode' ? boundary : load.call(this, name, ...args); };
  let api;
  try { api = require('../../dist/extension.js').activate(context); }
  finally { Module._load = load; }
  return {
    api, providers, output, warnings, warningCalls, status, resources, uncancelled, openedUris, candidateViews, statusItems, quickPicks,
    cancellationSource: () => new CancellationTokenSource(),
    executeCommand: boundary.commands.executeCommand,
    async open(name) {
      const fullPath = path.join(directory, name);
      const document = { uri: Uri.file(fullPath), languageId: 'fsharp', version: 1, isClosed: false, isDirty: false, text: await fs.readFile(fullPath, 'utf8'), getText() { return this.text; }, lineAt(line) { return { text: this.text.split('\n')[line] }; } };
      documents.push(document); events.open.fire(document); return document;
    },
    edit(document, text) { document.text = text; document.version++; document.isDirty = true; events.change.fire({ document }); },
    focus(document, position) { const selection = { active: position }; const editor = { document, selection, selections: [selection] }; boundary.window.activeTextEditor = editor; events.activeEditor.fire(editor); return editor; },
    move(position) { const editor = boundary.window.activeTextEditor; editor.selection = { active: position }; editor.selections = [editor.selection]; events.selection.fire({ textEditor: editor, selections: [editor.selection] }); },
    changeFolders() { events.folders.fire({}); },
    save(document) { document.isDirty = false; events.save.fire(document); },
    close(document) { document.isClosed = true; documents.splice(documents.indexOf(document), 1); events.close.fire(document); },
    position(document, line, name) { return new Position(line, document.lineAt(line).text.lastIndexOf(name) + 1); },
    waitForOutput(text) {
      if (output.join('').includes(text)) return Promise.resolve();
      return new Promise((resolve, reject) => {
        const subscription = events.output.event(() => {
          if (output.join('').includes(text)) { clearTimeout(timer); subscription.dispose(); resolve(); }
        });
        const timer = setTimeout(() => { subscription.dispose(); reject(new Error(`Timed out waiting for child output: ${text}`)); }, 5000);
      });
    },
    dispose() { for (const disposable of context.subscriptions.reverse()) disposable.dispose(); for (const event of Object.values(events)) event.dispose(); },
  };
};
