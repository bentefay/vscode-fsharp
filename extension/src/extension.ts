import * as vscode from 'vscode';
import * as path from 'node:path';
import { existsSync } from 'node:fs';
import { Backend } from './backend';
import { OverlayQueue } from './overlays';
import { MetadataDocuments, metadataScheme, locationUri, VirtualDocument } from './virtualDocuments';

type Mode = 'lexical' | 'semantic' | 'indexed';
interface Position { line: number; character: number }
interface Range { start: Position; end: Position }
interface Location { uri?: string; path?: string; range: Range }
interface QueryResult {
  locations?: Location[];
  documents?: VirtualDocument[];
  hover?: { markdown: string; range?: Range; isExternal?: boolean } | null;
  ownership?: 'external' | 'workspace' | 'unknown';
  completeness: 'exact' | 'approximate' | 'unavailable';
  source?: string;
  diagnostics?: string[];
  reason?: string;
  version?: number;
  projectContextId?: string;
}
interface TreeNode { id: string; label: string; kind: string; path?: string; uri?: string; children?: TreeNode[]; projectPath?: string; workspaceUri?: string }
interface TreeResult { nodes: TreeNode[]; completeness?: string; diagnostics?: string[] }
interface Session { backend: Backend; ready: Promise<unknown>; folder: vscode.WorkspaceFolder; overlays: OverlayQueue }

function range(value: Range): vscode.Range {
  return new vscode.Range(value.start.line, value.start.character, value.end.line, value.end.character);
}
function location(value: Location): vscode.Location {
  return new vscode.Location(locationUri(value), range(value.range));
}
function mode(resource?: vscode.Uri): Mode {
  const value = vscode.workspace.getConfiguration('fsharpNavigator', resource).get<string>('mode', 'indexed');
  return value === 'semantic' ? 'semantic' : value === 'lexical' || value === 'syntax' ? 'lexical' : 'indexed';
}

class Navigator implements vscode.Disposable {
  readonly output = vscode.window.createOutputChannel('F# Navigator');
  readonly status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 20);
  readonly sessions = new Map<string, Session>();
  private readonly subscriptions: vscode.Disposable[] = [];
  private readonly navigation = new Set<vscode.CancellationTokenSource>();
  private epoch = 0;
  private readonly selectedProjects = new Map<string, { projectPath: string; workspaceUri: string }>();
  private projectRefreshTimer: ReturnType<typeof setTimeout> | undefined;
  private lastWarning = 0;
  lastResult: QueryResult | undefined;

  constructor(private readonly context: vscode.ExtensionContext, private readonly refresh: () => void) {
    this.status.command = 'fsharpNavigator.selectMode';
    this.status.text = 'F#: indexed';
    this.status.tooltip = 'Rust navigation. Unresolved bindings are reported; no automatic compiler fallback.';
    this.status.show();
    this.subscriptions.push(
      vscode.workspace.onDidOpenTextDocument(document => this.changed(document)),
      vscode.workspace.onDidChangeTextDocument(event => this.changed(event.document)),
      vscode.workspace.onDidSaveTextDocument(document => this.changed(document)),
      vscode.workspace.onDidCloseTextDocument(document => {
        if (document.languageId !== 'fsharp' || document.uri.scheme !== 'file') return;
        this.epoch++;
        this.cancelNavigation();
        const session = this.existing(document.uri);
        session?.overlays.close(document.uri.fsPath);
        this.selectedProjects.delete(document.uri.toString());
      }),
      vscode.workspace.onDidChangeConfiguration(event => {
        if (event.affectsConfiguration('fsharpNavigator')) this.restart();
      }),
      vscode.workspace.onDidChangeWorkspaceFolders(() => this.restart())
    );
    // Source changes are sent as document overlays. Only project configuration changes refresh the tree.
    const projectChanged = () => {
      if (this.projectRefreshTimer) clearTimeout(this.projectRefreshTimer);
      this.projectRefreshTimer = setTimeout(() => {
        this.projectRefreshTimer = undefined;
        this.restart();
      }, 200);
    };
    const watcher = vscode.workspace.createFileSystemWatcher('**/*.{fsproj,sln,slnx,props,targets}');
    const assetsWatcher = vscode.workspace.createFileSystemWatcher('**/{global.json,project.assets.json}');
    this.subscriptions.push(watcher, watcher.onDidCreate(projectChanged), watcher.onDidChange(projectChanged), watcher.onDidDelete(projectChanged),
      assetsWatcher, assetsWatcher.onDidCreate(projectChanged), assetsWatcher.onDidChange(projectChanged), assetsWatcher.onDidDelete(projectChanged));
  }

  private folderFor(uri: vscode.Uri): vscode.WorkspaceFolder | undefined {
    const selected = this.selectedProjects.get(uri.toString());
    return (selected && vscode.workspace.workspaceFolders?.find(folder => folder.uri.toString() === selected.workspaceUri)) || vscode.workspace.getWorkspaceFolder(uri);
  }

  async openProjectFile(uri: vscode.Uri, projectPath: string, workspaceUri: string): Promise<void> {
    const key = uri.toString();
    const current = this.selectedProjects.get(key);
    if (current?.projectPath !== projectPath || current.workspaceUri !== workspaceUri) {
      this.epoch++;
      this.cancelNavigation();
      this.selectedProjects.delete(key);
      if (this.selectedProjects.size >= 256) this.selectedProjects.delete(this.selectedProjects.keys().next().value!);
      this.selectedProjects.set(key, { projectPath, workspaceUri });
    }
    await vscode.commands.executeCommand('vscode.open', uri);
  }

  private existing(uri: vscode.Uri): Session | undefined {
    const folder = this.folderFor(uri);
    return folder ? this.sessions.get(folder.uri.toString()) : undefined;
  }

  private changed(document: vscode.TextDocument): void {
    if (document.languageId !== 'fsharp' || document.uri.scheme !== 'file') return;
    this.epoch++;
    this.cancelNavigation();
    const session = this.existing(document.uri);
    if (session) {
      if (document.isDirty) session.overlays.schedule(document.uri.fsPath, document);
      else session.overlays.close(document.uri.fsPath);
    }
  }

  private cancelNavigation(): void {
    for (const request of this.navigation) request.cancel();
  }

  private executable(): string {
    const configured = vscode.workspace.getConfiguration('fsharpNavigator').get<string>('backendPath');
    if (configured) return configured;
    const suffix = process.platform === 'win32' ? '.exe' : '';
    const candidates = [
      path.join(this.context.extensionPath, 'bin', `${process.platform}-${process.arch}`, `fsharp-navigator${suffix}`),
      path.join(this.context.extensionPath, 'bin', `fsharp-navigator${suffix}`),
      path.join(this.context.extensionPath, 'target', 'release', `fsharp-navigator${suffix}`),
      path.join(this.context.extensionPath, 'target', 'debug', `fsharp-navigator${suffix}`),
    ];
    const binary = candidates.find(existsSync);
    if (!binary) throw new Error('F# Navigator engine is missing. Install a packaged extension or set fsharpNavigator.backendPath.');
    return binary;
  }

  async session(folder: vscode.WorkspaceFolder): Promise<Session> {
    if (!vscode.workspace.isTrusted) throw new Error('F# Navigator requires a trusted workspace before starting its engine.');
    let session = this.sessions.get(folder.uri.toString());
    if (session?.backend.closed) { session.overlays.dispose(); this.sessions.delete(folder.uri.toString()); session = undefined; }
    if (!session) {
      const config = vscode.workspace.getConfiguration('fsharpNavigator', folder.uri);
      const backend = new Backend(this.executable(), this.output);
      const helperPath = config.get<string>('helperPath') || undefined;
      const cacheDir = path.join(this.context.storageUri?.fsPath ?? this.context.globalStorageUri.fsPath, `workspace-${folder.index}`);
      const ready = backend.request('workspace/open', {
        root: folder.uri.fsPath, mode: mode(folder.uri), helperPath,
        helperArgs: config.get<string[]>('helperArgs', []), cacheDir,
        libraryCatalogPath: config.get<string>('libraryCatalogPath') || undefined,
        evaluatedModelPath: config.get<string>('evaluatedModelPath') || undefined,
        builtinEnvironmentPath: config.get<string>('builtinEnvironmentPath') || undefined,
        decompilerPath: config.get<string>('decompilerPath') || undefined,
        decompilerArgs: config.get<string[]>('decompilerArgs') ?? [],
        semanticSnapshotPath: config.get<string>('semanticSnapshotPath') || undefined,
        indexCacheMb: config.get<number>('indexCacheMb', 8),
        overlayBudgetMb: config.get<number>('overlayBudgetMb', 32),
        helperMemoryLimitMb: config.get<number>('helperMemoryLimitMb', 256),
      });
      const overlays = new OverlayQueue(ready, (method, params) => backend.request(method, params), error => this.report(error));
      session = { backend, ready, folder, overlays };
      this.sessions.set(folder.uri.toString(), session);
      ready.catch(error => {
        if (this.sessions.get(folder.uri.toString())?.backend === backend) this.sessions.delete(folder.uri.toString());
        overlays.dispose();
        backend.dispose();
        this.report(error);
      });
    }
    await session.ready;
    return session;
  }

  async query(method: string, document: vscode.TextDocument, position: vscode.Position, token: vscode.CancellationToken, includeDeclaration?: boolean): Promise<QueryResult | undefined> {
    // The cursor belongs to the version that invoked this provider, even if
    // initialization or an earlier overlay acknowledgement takes time.
    const version = document.version;
    const epoch = this.epoch;
    const folder = this.folderFor(document.uri);
    if (!folder) return undefined;
    const cancellation = new vscode.CancellationTokenSource();
    const inheritedCancellation = token.onCancellationRequested(() => cancellation.cancel());
    if (token.isCancellationRequested) cancellation.cancel();
    this.navigation.add(cancellation);
    try {
      const session = await this.session(folder);
      // Synchronize the cursor and dirty dependencies, not every clean open tab.
      for (const open of vscode.workspace.textDocuments) {
        if (open.languageId === 'fsharp' && open.uri.scheme === 'file' && (open === document || open.isDirty) && this.folderFor(open.uri)?.uri.toString() === folder.uri.toString()) {
          session.overlays.schedule(open.uri.fsPath, open);
        }
      }
      await session.overlays.flush();
      if (cancellation.token.isCancellationRequested || document.isClosed || document.version !== version || epoch !== this.epoch) return undefined;
      const result = await session.backend.request<QueryResult>(method, {
        path: document.uri.fsPath, line: position.line, character: position.character,
        version, includeDeclaration, projectPath: this.selectedProjects.get(document.uri.toString())?.projectPath,
      }, cancellation.token, mode(folder.uri) === 'semantic' ? 120_000 : 30_000);
      if (cancellation.token.isCancellationRequested || document.isClosed || document.version !== version || epoch !== this.epoch || (result.version !== undefined && result.version !== version)) return undefined;
      this.lastResult = result;
      if (result.completeness !== 'exact') {
        const details = result.reason || result.diagnostics?.join('; ') || 'This binding is not supported by the current engine.';
        this.status.text = 'F#: incomplete';
        this.status.tooltip = details;
        this.output.appendLine(`${method}: ${result.completeness} (${result.source ?? 'unknown'}): ${details}`);
        if (method !== 'navigation/hover' && Date.now() - this.lastWarning > 3000) {
          this.lastWarning = Date.now();
          void vscode.window.showWarningMessage(`F# Navigator could not verify a complete result. ${details}`, 'Show Details').then(action => {
            if (action) this.output.show(true);
          });
        }
        return undefined;
      }
      this.status.text = `F#: ${mode(folder.uri)}`;
      this.status.tooltip = `Verified by ${result.source ?? mode(folder.uri)}. Compiler analysis is never started automatically.`;
      return result;
    } catch (error) {
      if (!(error instanceof vscode.CancellationError)) this.report(error);
      return undefined;
    } finally {
      this.navigation.delete(cancellation);
      inheritedCancellation.dispose();
      cancellation.dispose();
    }
  }

  report(error: unknown): void {
    const message = error instanceof Error ? error.message : String(error);
    this.status.text = 'F#: unavailable';
    this.status.tooltip = message;
    this.output.appendLine(message);
  }

  restart(): void {
    this.epoch++;
    this.cancelNavigation();
    for (const session of this.sessions.values()) { session.overlays.dispose(); session.backend.dispose(); }
    this.sessions.clear();
    this.selectedProjects.clear();
    this.status.text = `F#: ${mode()}`;
    this.refresh();
  }

  dispose(): void {
    if (this.projectRefreshTimer) clearTimeout(this.projectRefreshTimer);
    this.restart();
    for (const subscription of this.subscriptions) subscription.dispose();
    this.status.dispose();
    this.output.dispose();
  }
}

class SolutionTree implements vscode.TreeDataProvider<TreeNode> {
  private readonly emitter = new vscode.EventEmitter<TreeNode | undefined>();
  readonly onDidChangeTreeData = this.emitter.event;
  navigator!: Navigator;
  refresh(): void { this.emitter.fire(undefined); }
  getTreeItem(node: TreeNode): vscode.TreeItem {
    const item = new vscode.TreeItem(node.label, node.children?.length ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None);
    item.id = node.id;
    item.contextValue = node.kind;
    if (node.path || node.uri) {
      item.resourceUri = node.uri ? vscode.Uri.parse(node.uri) : vscode.Uri.file(node.path!);
      item.tooltip = item.resourceUri.fsPath;
      if (!node.children?.length) {
        item.command = node.kind === 'file' && node.projectPath && node.workspaceUri
          ? { command: 'fsharpNavigator.openProjectFile', title: 'Open in Project', arguments: [item.resourceUri, node.projectPath, node.workspaceUri] }
          : { command: 'vscode.open', title: 'Open', arguments: [item.resourceUri] };
        if (node.projectPath) item.tooltip = `${item.resourceUri.fsPath}\nProject: ${node.projectPath}`;
      }
    }
    if (node.kind === 'project') item.iconPath = new vscode.ThemeIcon('project');
    else if (node.kind === 'solution') item.iconPath = new vscode.ThemeIcon('symbol-namespace');
    else if (node.kind === 'folder') item.iconPath = new vscode.ThemeIcon('folder');
    return item;
  }
  async getChildren(node?: TreeNode): Promise<TreeNode[]> {
    if (node) return node.children ?? [];
    const nodes: TreeNode[] = [];
    for (const folder of vscode.workspace.workspaceFolders ?? []) {
      try {
        const session = await this.navigator.session(folder);
        const result = await session.backend.request<TreeResult | TreeNode[]>('workspace/tree');
        const children = Array.isArray(result) ? result : result.nodes;
        const attachContext = (node: TreeNode, projectPath?: string): TreeNode => {
          const owningProject = node.kind === 'project' ? node.path : projectPath;
          return { ...node, projectPath: owningProject, workspaceUri: folder.uri.toString(), children: node.children?.map(child => attachContext(child, owningProject)) };
        };
        for (const child of children ?? []) nodes.push(attachContext(child));
        if (!Array.isArray(result) && result.diagnostics?.length) this.navigator.output.appendLine(result.diagnostics.join('\n'));
      } catch (error) { this.navigator.report(error); }
    }
    return nodes;
  }
  dispose(): void { this.emitter.dispose(); }
}

export function activate(context: vscode.ExtensionContext) {
  const tree = new SolutionTree();
  const metadata = new MetadataDocuments();
  const navigator = new Navigator(context, () => tree.refresh());
  tree.navigator = navigator;
  const selector: vscode.DocumentSelector = { language: 'fsharp', scheme: 'file' };
  context.subscriptions.push(navigator, tree, metadata,
    vscode.workspace.registerTextDocumentContentProvider(metadataScheme, metadata),
    vscode.window.registerTreeDataProvider('fsharpNavigator.solution', tree),
    vscode.languages.registerDefinitionProvider(selector, {
      async provideDefinition(document, position, token) {
        const result = await navigator.query('navigation/definition', document, position, token);
        if (!result) return undefined;
        try {
          metadata.accept(result.documents);
          const locations = result.locations?.map(location);
          for (const target of locations ?? []) {
            if (target.uri.scheme === metadataScheme && !metadata.has(target.uri)) {
              throw new Error('Engine returned an external definition without its signature document.');
            }
          }
          return locations;
        } catch (error) { navigator.report(error); return undefined; }
      }
    }),
    vscode.languages.registerReferenceProvider(selector, {
      async provideReferences(document, position, referenceContext, token) {
        const result = await navigator.query('navigation/references', document, position, token, referenceContext.includeDeclaration);
        return result?.locations?.map(location);
      }
    }),
    vscode.languages.registerHoverProvider(selector, {
      async provideHover(document, position, token) {
        const result = await navigator.query('navigation/hover', document, position, token);
        if (!result?.hover || (result.ownership !== 'external' && result.hover.isExternal !== true)) return undefined;
        const markdown = new vscode.MarkdownString(result.hover.markdown);
        markdown.isTrusted = false;
        markdown.supportHtml = false;
        return new vscode.Hover(markdown, result.hover.range ? range(result.hover.range) : undefined);
      }
    }),
    vscode.commands.registerCommand('fsharpNavigator.openProjectFile', (uri: vscode.Uri, projectPath: string, workspaceUri: string) => navigator.openProjectFile(uri, projectPath, workspaceUri)),
    vscode.commands.registerCommand('fsharpNavigator.refresh', () => tree.refresh()),
    vscode.commands.registerCommand('fsharpNavigator.restart', () => navigator.restart()),
    vscode.commands.registerCommand('fsharpNavigator.selectMode', async () => {
      const choice = await vscode.window.showQuickPick([
        { label: 'Indexed', description: 'Rust index; no automatic compiler analysis', value: 'indexed' },
        { label: 'Lexical', description: 'Rust syntax and scope analysis', value: 'lexical' },
        { label: 'Semantic (experimental)', description: 'Explicit .NET compiler helper; potentially expensive', value: 'semantic' },
      ], { title: 'F# Navigator mode' });
      if (choice) await vscode.workspace.getConfiguration('fsharpNavigator').update('mode', choice.value, vscode.ConfigurationTarget.Workspace);
    })
  );
  // Export observable state for the real extension-host tests, not an alternate analysis implementation.
  return {
    tree,
    async stats() {
      const results: unknown[] = [];
      for (const session of navigator.sessions.values()) results.push(await session.backend.request('workspace/stats'));
      return results;
    },
    get lastResult() { return navigator.lastResult; },
    get processIds() { return [...navigator.sessions.values()].map(session => session.backend.pid); },
    restart() { navigator.restart(); },
  };
}

export function deactivate(): void { /* Extension subscriptions own process disposal. */ }
