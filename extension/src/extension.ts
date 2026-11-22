import * as vscode from 'vscode';
import * as path from 'node:path';
import { existsSync } from 'node:fs';
import { Backend } from './backend';
import { OverlayQueue } from './overlays';
import { trace, traceEnabled, traceEvents, demandTraceConfig } from './trace';
import { PossibleReferences, OccurrenceResult, occurrenceLocation, sameIdentity, sameLocation, validOccurrenceResult } from './possibleReferences';
import { MetadataDocuments, metadataScheme, locationUri, VirtualDocument } from './virtualDocuments';

type Mode = 'lexical' | 'semantic' | 'indexed';
interface Position { line: number; character: number }
interface Range { start: Position; end: Position }
interface Location { uri?: string; path?: string; range: Range }
interface QueryResult {
  locations?: Location[];
  candidates?: Location[];
  likelyCandidates?: Location[];
  likelyCandidateTotal?: number;
  candidateRanking?: string;
  candidateEvidence?: { location: Location; evidence: string[] }[];
  indexing?: { phase: 'scanning' | 'indexing'; retryAfterMs?: number; generation?: number };
  candidateTruncated?: boolean;
  candidateTruncationReason?: string;
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
interface TreeResult { nodes: TreeNode[]; projects?: { files: string[] }[]; completeness?: string; diagnostics?: string[] }
interface Session { backend: Backend; ready: Promise<unknown>; folder: vscode.WorkspaceFolder; overlays: OverlayQueue; sourceFiles: Set<string> }

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

function cancellable<T>(operation: PromiseLike<T>, token: vscode.CancellationToken): Promise<T> {
  return new Promise((resolve, reject) => {
    const subscription = token.onCancellationRequested(() => reject(new vscode.CancellationError()));
    if (token.isCancellationRequested) { subscription.dispose(); reject(new vscode.CancellationError()); return; }
    Promise.resolve(operation).then(resolve, reject).finally(() => subscription.dispose());
  });
}

function pause(milliseconds: number, token: vscode.CancellationToken): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { subscription.dispose(); resolve(); }, milliseconds);
    const subscription = token.onCancellationRequested(() => { clearTimeout(timer); subscription.dispose(); reject(new vscode.CancellationError()); });
    if (token.isCancellationRequested) { clearTimeout(timer); subscription.dispose(); reject(new vscode.CancellationError()); }
  });
}

// Candidate offers never enter the exact provider result or publish rows automatically.
type CandidateChoice = vscode.QuickPickItem & { readonly target: vscode.Location };
class DefinitionCandidates implements vscode.Disposable {
  private items: vscode.Location[] = [];
  private allItems: vscode.Location[] = [];
  private reason = '';
  private readonly evidence = new Map<vscode.Location, string[]>();
  private validate: ((target: vscode.Location, token?: vscode.CancellationToken) => Promise<boolean>) | undefined;
  private generation = 0;
  private offer: vscode.StatusBarItem | undefined;
  private picker: vscode.QuickPick<CandidateChoice> | undefined;
  private selection: vscode.CancellationTokenSource | undefined;
  private pickerSubscriptions: vscode.Disposable[] = [];
  get visible(): boolean { return !!this.offer; }
  getChildren(): vscode.Location[] { return this.items; }
  getAllCandidates(): vscode.Location[] { return this.allItems; }
  clear(): void {
    this.generation++;
    this.selection?.cancel(); this.selection?.dispose(); this.selection = undefined;
    const picker = this.picker; this.picker = undefined;
    for (const subscription of this.pickerSubscriptions.splice(0)) subscription.dispose();
    picker?.hide(); picker?.dispose();
    this.offer?.hide(); this.offer?.dispose(); this.offer = undefined;
    this.items = []; this.allItems = []; this.evidence.clear(); this.validate = undefined;
  }
  async show(values: Location[], reason: string, requestId: number | undefined, validate: (target: vscode.Location, token?: vscode.CancellationToken) => Promise<boolean>, ranking?: QueryResult): Promise<void> {
    this.clear();
    const same = (a: vscode.Location, b: vscode.Location) => a.uri.toString() === b.uri.toString()
      && a.range.start.line === b.range.start.line && a.range.start.character === b.range.start.character
      && a.range.end.line === b.range.end.line && a.range.end.character === b.range.end.character;
    for (const value of values.map(location)) if (!this.allItems.some(item => same(item, value))) this.allItems.push(value);
    const prioritized = ranking?.likelyCandidates?.map(location) || [];
    this.items = [...new Set(prioritized.map(value => this.allItems.find(item => same(item, value))).filter((item): item is vscode.Location => !!item))];
    if (!this.items.length) this.items = [...this.allItems];
    this.reason = reason;
    for (const hint of ranking?.candidateEvidence || []) {
      const item = this.allItems.find(value => same(value, location(hint.location)));
      if (item) this.evidence.set(item, hint.evidence);
    }
    this.validate = validate;
    if (!this.allItems.length) return;
    this.offer = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 19);
    this.offer.name = 'F# Navigator possible definitions';
    this.offer.text = `F#: ${this.allItems.length} possible definitions (choose)`;
    this.offer.tooltip = `Binding unverified; collected matches may be incomplete. ${reason}`;
    this.offer.command = { command: 'fsharpNavigator.chooseCandidates', title: 'Choose Possible Definition', arguments: [this.generation] };
    this.offer.show();
    if (traceEnabled) trace('candidateOffer', { requestId, generation: this.generation, count: this.allItems.length });
  }
  choose(generation: unknown = this.generation): void {
    if (typeof generation !== 'number' || generation !== this.generation || !this.allItems.length || !this.validate || this.picker) return;
    const validate = this.validate;
    const selection = new vscode.CancellationTokenSource(); this.selection = selection;
    const picker = vscode.window.createQuickPick<CandidateChoice>(); this.picker = picker;
    const labels: Record<string, string> = {
      'same-project': 'Same project', 'same-source': 'Same file',
      'explicit-type-spelling': 'Matching written type', 'record-label-shape': 'Matching record fields',
      'type-position-and-declaration': 'Type declaration in a type position',
      'alias-rhs-declaration-shape': 'Matching alias target shape',
      'qualifier-spelling': 'Matching qualifier', 'enclosing-lexical-scope': 'Enclosing scope',
    };
    const ordered = [...this.items, ...this.allItems.filter(item => !this.items.includes(item))];
    picker.items = ordered.map(target => ({ target,
      label: `${path.basename(target.uri.fsPath)}:${target.range.start.line + 1}:${target.range.start.character + 1}`,
      description: target.uri.fsPath,
      detail: `Binding unverified${this.evidence.get(target)?.length ? ` — ${this.evidence.get(target)!.map(value => labels[value]).filter(Boolean).join(', ')}` : ''}`,
    }));
    picker.title = `Possible definitions — ${ordered.length} collected matches (incomplete)`;
    picker.placeholder = `Choose explicitly; binding unverified. ${this.reason}`;
    picker.matchOnDescription = true; picker.matchOnDetail = true;
    let accepting = false;
    const current = () => generation === this.generation && this.picker === picker && !selection.token.isCancellationRequested;
    const hide = picker.onDidHide(() => { if (this.picker === picker) this.clear(); });
    const accept = picker.onDidAccept(() => {
      if (accepting || !current() || picker.selectedItems.length !== 1) return;
      const selected = picker.selectedItems[0];
      if (!picker.items.includes(selected)) return;
      accepting = true; picker.busy = true;
      void (async () => {
        try {
          const valid = await validate(selected.target, selection.token);
          if (!current()) return;
          if (!valid || picker.selectedItems.length !== 1 || picker.selectedItems[0] !== selected) { this.clear(); return; }
          // No await between the final identity/cancellation check and opening.
          await vscode.commands.executeCommand('vscode.open', selected.target.uri, { selection: selected.target.range });
          if (current()) this.clear();
        } catch { if (current()) this.clear(); }
      })();
    });
    this.pickerSubscriptions.push(hide, accept);
    picker.show();
  }
  dispose(): void { this.clear(); }
}

const incompleteReferencesMessage = 'Scope was not fully searched. “No references found” is not evidence that there are no uses.';

type ReferenceNotice = {
  key: string; editor: vscode.TextEditor | undefined; request: number;
  pending: boolean; valid: boolean; cancellation?: vscode.Disposable;
};

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
  private referenceRequest = 0;
  private referenceContext = 0;
  private referenceEditor: vscode.TextEditor | undefined;
  private referenceSelection = '';
  private referenceNotice: ReferenceNotice | undefined;
  private definitionRequest = 0;
  private activeDefinition: vscode.CancellationTokenSource | undefined;
  private definitionFlight: { key: string; requestId: number; editor: vscode.TextEditor; cancellation: vscode.CancellationTokenSource; promise: Promise<QueryResult | undefined> } | undefined;
  private occurrenceRequest = 0;
  private occurrenceEpoch = 0;
  private occurrenceCancellation: vscode.CancellationTokenSource | undefined;
  lastPossibleReferences: OccurrenceResult | undefined;
  lastResult: QueryResult | undefined;

  constructor(private readonly context: vscode.ExtensionContext, private readonly refresh: () => void, readonly candidates: DefinitionCandidates, readonly possibleReferences: PossibleReferences) {
    this.status.command = 'fsharpNavigator.selectMode';
    this.status.text = 'F#: indexed';
    this.status.tooltip = 'Rust navigation. Unresolved bindings are reported; no automatic compiler fallback.';
    this.status.show();
    this.subscriptions.push(
      vscode.window.onDidChangeTextEditorSelection(() => { this.candidates.clear(); this.observeReferenceContext(); }),
      vscode.window.onDidChangeActiveTextEditor(() => { this.candidates.clear(); this.observeReferenceContext(); }),
      vscode.workspace.onDidOpenTextDocument(document => this.changed(document, true)),
      vscode.workspace.onDidChangeTextDocument(event => this.changed(event.document)),
      vscode.workspace.onDidSaveTextDocument(document => this.changed(document)),
      vscode.workspace.onDidCloseTextDocument(document => {
        if (document.languageId !== 'fsharp' || document.uri.scheme !== 'file') return;
        this.epoch++;
        this.cancelNavigation();
        for (const session of this.sessions.values()) session.overlays.close(document.uri.fsPath);
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

  private changed(document: vscode.TextDocument, opened = false): void {
    if (document.languageId !== 'fsharp' || document.uri.scheme !== 'file') return;
    this.epoch++;
    this.cancelNavigation(!(opened && !document.isDirty));
    const owner = this.folderFor(document.uri)?.uri.toString();
    for (const session of this.sessions.values()) {
      if (!document.isDirty) session.overlays.close(document.uri.fsPath);
      else if (owner === session.folder.uri.toString() || session.sourceFiles.has(document.uri.toString())) {
        session.overlays.schedule(document.uri.fsPath, document);
      }
    }
  }

  private cancelNavigation(invalidateOccurrences = true): void {
    this.invalidateReferenceNotice();
    this.candidates.clear();
    if (invalidateOccurrences) { this.occurrenceEpoch++; this.occurrenceCancellation?.cancel(); this.possibleReferences.clear(); }
    for (const request of this.navigation) request.cancel();
  }
  cancelDefinition(): void {
    if (!this.activeDefinition) return;
    this.activeDefinition.cancel(); this.candidates.clear();
  }

  private async occurrenceOperation<T>(editor: vscode.TextEditor | undefined, clear: boolean, work: (token: vscode.CancellationToken) => Promise<T>): Promise<T | undefined> {
    this.occurrenceCancellation?.cancel();
    const request = ++this.occurrenceRequest;
    if (clear) this.possibleReferences.clear('Searching a limited set of files for same-name occurrences; results will be incomplete.');
    const cancellation = new vscode.CancellationTokenSource();
    this.occurrenceCancellation = cancellation;
    const position = editor?.selection.active;
    const caret = vscode.window.onDidChangeTextEditorSelection(event => {
      if (event.textEditor === editor && (event.selections.length !== 1 || !position?.isEqual(event.selections[0].active))) cancellation.cancel();
    });
    const active = vscode.window.onDidChangeActiveTextEditor(current => { if (current !== editor) cancellation.cancel(); });
    const deadline = setTimeout(() => cancellation.cancel(), 5000);
    try {
      return await cancellable(vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'F# Navigator: possible references (incomplete)', cancellable: true }, async (_progress, token) => {
        const inherited = token.onCancellationRequested(() => cancellation.cancel());
        if (token.isCancellationRequested) cancellation.cancel();
        try {
          if (request !== this.occurrenceRequest || cancellation.token.isCancellationRequested
            || vscode.window.activeTextEditor !== editor || (editor && (!position?.isEqual(editor.selection.active) || editor.selections.length !== 1))) return undefined;
          return await cancellable(work(cancellation.token), cancellation.token);
        } finally { inherited.dispose(); }
      }), cancellation.token);
    } catch (error) { if (!(error instanceof vscode.CancellationError)) this.report(error); return undefined; }
    finally {
      clearTimeout(deadline); caret.dispose(); active.dispose();
      if (request === this.occurrenceRequest) {
        this.occurrenceCancellation = undefined;
        if (cancellation.token.isCancellationRequested) this.possibleReferences.clear('Possible-reference search cancelled or its five-second budget ended. No complete reference set has been established.');
      }
      cancellation.dispose();
    }
  }

  private observeReferenceContext(): void {
    const editor = vscode.window.activeTextEditor;
    const selection = JSON.stringify(editor?.selections.map(value => [value.active.line, value.active.character]) ?? []);
    if (editor === this.referenceEditor && selection === this.referenceSelection) return;
    this.referenceEditor = editor; this.referenceSelection = selection;
    this.referenceContext++; this.invalidateReferenceNotice();
  }

  private invalidateReferenceNotice(): void {
    if (!this.referenceNotice) return;
    this.referenceNotice.valid = false;
    this.referenceNotice.cancellation?.dispose(); this.referenceNotice.cancellation = undefined;
    // VS Code cannot dismiss our notification. Keep its pending slot occupied
    // so unrelated retries cannot accumulate obsolete, still-visible actions.
  }

  private offerPossibleReferences(document: vscode.TextDocument, position: vscode.Position,
    editor: vscode.TextEditor | undefined, version: number, epoch: number, context: number,
    request: number, token: vscode.CancellationToken, details: string): void {
    const folder = this.folderFor(document.uri);
    if (!folder || token.isCancellationRequested || request !== this.referenceRequest || context !== this.referenceContext) return;
    const project = this.selectedProjects.get(document.uri.toString())?.projectPath;
    const key = JSON.stringify([document.uri.toString(), version, position.line, position.character, epoch, context, folder.uri.toString(), project]);
    let notice = this.referenceNotice;
    if (notice?.pending) {
      if (!notice.valid || notice.key !== key || notice.editor !== editor) return;
      notice.request = request;
      notice.cancellation?.dispose();
      notice.cancellation = token.onCancellationRequested(() => {
        if (this.referenceNotice === notice && notice?.request === request) this.invalidateReferenceNotice();
      });
      if (token.isCancellationRequested) this.invalidateReferenceNotice();
      return;
    }
    notice = { key, editor, request, pending: true, valid: true };
    this.referenceNotice = notice;
    const captured = notice;
    captured.cancellation = token.onCancellationRequested(() => {
      if (this.referenceNotice === captured && captured.request === request) this.invalidateReferenceNotice();
    });
    if (token.isCancellationRequested) { this.invalidateReferenceNotice(); captured.pending = false; return; }
    const current = () => this.referenceNotice === captured && captured.valid && captured.request === this.referenceRequest
      && context === this.referenceContext && epoch === this.epoch && !document.isClosed && document.version === version
      && !!editor && vscode.window.activeTextEditor === editor && editor.document === document
      && editor.selections.length === 1 && editor.selection.active.isEqual(position)
      && this.folderFor(document.uri)?.uri.toString() === folder.uri.toString()
      && this.selectedProjects.get(document.uri.toString())?.projectPath === project;
    void Promise.resolve(vscode.window.showWarningMessage(
      `${incompleteReferencesMessage} ${details}`,
      'Show Possible References',
    )).then(async action => {
      if (action !== 'Show Possible References' || !current()) return;
      captured.valid = false; // one explicit transaction; no automatic search
      await this.showPossibleReferences();
    }).catch(error => { if (current()) this.report(error); }).finally(() => {
      captured.pending = false; captured.valid = false;
      captured.cancellation?.dispose(); captured.cancellation = undefined;
    });
  }

  private async synchronizeOccurrences(session: Session, document: vscode.TextDocument, token: vscode.CancellationToken): Promise<void> {
    await cancellable(this.synchronize(session, document), token);
    if (token.isCancellationRequested) throw new vscode.CancellationError();
    // Ordinary providers keep their existing overlay policy. This explicit disk
    // occurrence search must not mistake an old clean buffer for current disk.
    for (const open of vscode.workspace.textDocuments) {
      if (open.languageId === 'fsharp' && open.uri.scheme === 'file' && !open.isDirty) session.overlays.close(open.uri.fsPath);
    }
    await cancellable(session.overlays.flush(), token);
  }

  async showPossibleReferences(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'fsharp' || editor.document.uri.scheme !== 'file') return;
    const document = editor.document, position = editor.selection.active;
    const folder = this.folderFor(document.uri);
    if (!folder) return;
    if (mode(folder.uri) !== 'indexed') {
      await vscode.window.showWarningMessage('Possible references are available only in Indexed mode. Select Indexed mode explicitly to use this limited same-name search.');
      return;
    }
    const sourceText = document.getText();
    if (Buffer.byteLength(sourceText, 'utf8') > 1024 * 1024) {
      await vscode.window.showWarningMessage('Possible references cannot scan an origin document larger than 1 MiB. No complete reference set has been established.');
      return;
    }
    const version = document.version, epoch = this.occurrenceEpoch;
    const current = () => !document.isClosed && document.version === version && this.occurrenceEpoch === epoch;
    const params = { path: document.uri.fsPath, line: position.line, character: position.character, version: document.isDirty ? version : undefined, expectedSourceText: sourceText,
      projectPath: this.selectedProjects.get(document.uri.toString())?.projectPath };
    await this.occurrenceOperation(editor, true, async token => {
      const session = await cancellable(this.session(folder), token);
      await cancellable(this.synchronizeOccurrences(session, document, token), token);
      if (!current() || token.isCancellationRequested) return;
      const result = await session.backend.request<OccurrenceResult>('navigation/possibleReferences', params, token);
      if (!current() || token.isCancellationRequested || (result.version != null && result.version !== version)) return;
      if (!validOccurrenceResult(result) || result.occurrenceIdentity.origin.uri !== document.uri.toString()) throw new Error('Possible references did not return a labelled incomplete occurrence result.');
      this.lastPossibleReferences = result;
      await cancellable(this.possibleReferences.show(result, async row => {
        if (!current()) return false;
        const validated = await this.occurrenceOperation(vscode.window.activeTextEditor, false, async clickToken => {
          if (!current()) return false;
          await cancellable(this.synchronizeOccurrences(session, document, clickToken), clickToken);
          if (!current() || clickToken.isCancellationRequested) return false;
          const fresh = await session.backend.request<OccurrenceResult>('navigation/possibleReferences', params, clickToken);
          return current() && !clickToken.isCancellationRequested && validOccurrenceResult(fresh)
            && sameIdentity(result.occurrenceIdentity.origin, fresh.occurrenceIdentity.origin)
            && sameIdentity(row.identity, fresh.occurrenceIdentity.targets.find(target => target.uri === row.location.uri.toString()))
            && !!fresh.candidates?.some(value => sameLocation(row.location, occurrenceLocation(value)));
        });
        return validated === true && current();
      }), token);
      if (!current() || token.isCancellationRequested) return;
      this.status.text = 'F#: possible references (incomplete)';
      this.status.tooltip = 'Same-name occurrences may include declarations and unrelated symbols. Only a limited set of files was scanned; this is not a complete reference set.';
    });
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
      if (traceEnabled) trace('workspaceOpenSent', { pid: backend.pid });
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
        ...(demandTraceConfig ? { diagnosticModelOrdinals: demandTraceConfig.modelOrdinals } : undefined),
      });
      const overlays = new OverlayQueue(ready, (method, params) => backend.request(method, params), error => this.report(error));
      session = { backend, ready, folder, overlays, sourceFiles: new Set() };
      this.sessions.set(folder.uri.toString(), session);
      if (traceEnabled) void ready.then(() => trace('workspaceOpenReady', { pid: backend.pid }), () => {});
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

  private async synchronize(session: Session, cursor?: vscode.TextDocument): Promise<void> {
    // A linked dependency can be outside the workspace folder (or in another
    // root). Resolve foreign dirty buffers from the native project membership,
    // lazily, so ordinary local navigation does not load the project tree here.
    const documents = vscode.workspace.textDocuments.filter(open =>
      open.languageId === 'fsharp' && open.uri.scheme === 'file' && (open === cursor || open.isDirty));
    const owns = (open: vscode.TextDocument) => this.folderFor(open.uri)?.uri.toString() === session.folder.uri.toString();
    const foreign = documents.filter(open => open.isDirty && !owns(open));
    if (foreign.length) {
      // Refresh each time: a cached negative cannot prove exclusion after an
      // externally changed project model. Failure must suppress the query.
      const tree = await session.backend.request<TreeResult>('workspace/tree', {});
      const sourceFiles = new Set((tree.projects ?? []).flatMap(project => project.files).map(file => vscode.Uri.file(file).toString()));
      if (tree.completeness === 'exact' && tree.projects) {
        for (const open of foreign) {
          if (!sourceFiles.has(open.uri.toString())) session.overlays.close(open.uri.fsPath);
        }
      }
      session.sourceFiles = sourceFiles;
      if (foreign.some(open => !session.sourceFiles.has(open.uri.toString())) &&
          (tree.completeness !== 'exact' || !tree.projects)) {
        throw new Error('Cannot exclude a dirty external F# buffer from the current project. Navigation is unavailable until project membership is known.');
      }
    }
    for (const open of documents) {
      if (owns(open) || session.sourceFiles.has(open.uri.toString())) session.overlays.schedule(open.uri.fsPath, open);
    }
    await session.overlays.flush();
  }

  async indexWorkspace(): Promise<void> {
    for (const folder of vscode.workspace.workspaceFolders ?? []) {
      const session = await this.session(folder);
      await this.synchronize(session);
      if (traceEnabled) trace('workspaceIndexStart');
      await session.backend.request('workspace/index', {}, undefined, 120_000);
      if (traceEnabled) trace('workspaceIndexReady');
    }
  }

  query(method: string, document: vscode.TextDocument, position: vscode.Position, token: vscode.CancellationToken, includeDeclaration?: boolean): Promise<QueryResult | undefined> {
    if (method === 'navigation/definition') this.occurrenceCancellation?.cancel();
    const editor = vscode.window.activeTextEditor;
    const folder = this.folderFor(document.uri);
    const eligible = method === 'navigation/definition' && folder && mode(folder.uri) === 'indexed'
      && editor?.document === document && editor.selections.length === 1 && editor.selection.active.isEqual(position);
    if (!eligible || !editor) return this.queryOnce(method, document, position, token, includeDeclaration);
    const key = JSON.stringify([document.uri.toString(), document.version, position.line, position.character,
      this.epoch, folder.uri.toString(), this.selectedProjects.get(document.uri.toString())?.projectPath]);
    const previous = this.definitionFlight;
    if (previous && previous.requestId === this.definitionRequest && previous.key === key && previous.editor === editor
      && !previous.cancellation.token.isCancellationRequested && !this.activeDefinition?.token.isCancellationRequested) {
      if (traceEnabled) trace('definitionJoined', { requestId: this.definitionRequest });
      return this.joinDefinition(previous, token);
    }
    const cancellation = new vscode.CancellationTokenSource();
    if (token.isCancellationRequested) cancellation.cancel();
    const promise = this.queryOnce(method, document, position, cancellation.token, includeDeclaration);
    const flight = { key, requestId: this.definitionRequest, editor, cancellation, promise };
    this.definitionFlight = flight;
    flight.promise = flight.promise.finally(() => {
      if (this.definitionFlight === flight) this.definitionFlight = undefined;
      cancellation.dispose();
    });
    return this.joinDefinition(flight, token);
  }

  private joinDefinition(flight: NonNullable<Navigator['definitionFlight']>, token: vscode.CancellationToken): Promise<QueryResult | undefined> {
    const listener = token.onCancellationRequested(() => flight.cancellation.cancel());
    if (token.isCancellationRequested) flight.cancellation.cancel();
    return flight.promise.finally(() => listener.dispose());
  }

  private async queryOnce(method: string, document: vscode.TextDocument, position: vscode.Position, token: vscode.CancellationToken, includeDeclaration?: boolean): Promise<QueryResult | undefined> {
    // The cursor belongs to the version that invoked this provider, even if
    // initialization or an earlier overlay acknowledgement takes time.
    const definitionRequest = method === 'navigation/definition' ? ++this.definitionRequest : undefined;
    this.observeReferenceContext();
    const referenceRequest = method === 'navigation/references' ? ++this.referenceRequest : undefined;
    const referenceContext = this.referenceContext;
    if (definitionRequest !== undefined) { this.referenceRequest++; this.invalidateReferenceNotice(); }
    if (definitionRequest !== undefined) this.activeDefinition?.cancel();
    if (traceEnabled) trace('providerStart', { requestId: definitionRequest, method });
    if (definitionRequest !== undefined) this.candidates.clear();
    const version = document.version;
    const epoch = this.epoch;
    const folder = this.folderFor(document.uri);
    if (!folder) return undefined;
    const cancellation = new vscode.CancellationTokenSource();
    if (definitionRequest !== undefined) this.activeDefinition = cancellation;
    let timedOut = false;
    let continuing = false;
    const deadline = definitionRequest !== undefined && mode(folder.uri) === 'indexed'
      ? setTimeout(() => { timedOut = true; cancellation.cancel(); }, 3000) : undefined;
    const entryEditor = vscode.window.activeTextEditor;
    if (referenceRequest !== undefined && this.referenceNotice) {
      const key = JSON.stringify([document.uri.toString(), version, position.line, position.character, epoch,
        referenceContext, folder.uri.toString(), this.selectedProjects.get(document.uri.toString())?.projectPath]);
      if (this.referenceNotice.key !== key || this.referenceNotice.editor !== entryEditor) this.invalidateReferenceNotice();
    }
    const followsCaret = definitionRequest !== undefined && entryEditor?.document.uri.toString() === document.uri.toString()
      && entryEditor.selection.active.isEqual(position);
    const caretChanged = followsCaret ? vscode.window.onDidChangeTextEditorSelection(event => {
      if (event.textEditor !== entryEditor) return;
      if (event.selections.length !== 1 || !event.selections[0].active.isEqual(position)) cancellation.cancel();
    }) : undefined;
    const editorChanged = followsCaret ? vscode.window.onDidChangeActiveTextEditor(editor => {
      if (editor !== entryEditor) cancellation.cancel();
    }) : undefined;
    const inheritedCancellation = token.onCancellationRequested(() => {
      if (traceEnabled) trace('providerCancellation', { requestId: definitionRequest, method });
      cancellation.cancel();
      if (referenceRequest !== undefined && referenceRequest === this.referenceRequest) this.invalidateReferenceNotice();
      if (definitionRequest !== undefined && definitionRequest === this.definitionRequest) this.candidates.clear();
    });
    if (token.isCancellationRequested) {
      cancellation.cancel();
      if (referenceRequest !== undefined) this.invalidateReferenceNotice();
    }
    this.navigation.add(cancellation);
    try {
      if (cancellation.token.isCancellationRequested) return undefined;
      const session = await cancellable(this.session(folder), cancellation.token);
      await cancellable(this.synchronize(session, document), cancellation.token);
      if (traceEnabled) trace('overlaysReady', { requestId: definitionRequest, method });
      if (cancellation.token.isCancellationRequested || document.isClosed || document.version !== version || epoch !== this.epoch) return undefined;
      const params = {
        path: document.uri.fsPath, line: position.line, character: position.character,
        version, includeDeclaration, projectPath: this.selectedProjects.get(document.uri.toString())?.projectPath,
        ...(demandTraceConfig && definitionRequest !== undefined ? { diagnosticWatchSlot: demandTraceConfig.watchSlot } : undefined),
      };
      let result = await cancellable(session.backend.request<QueryResult>(method, params,
        cancellation.token, mode(folder.uri) === 'semantic' ? 120_000 : 30_000, definitionRequest), cancellation.token);
      while (definitionRequest !== undefined && mode(folder.uri) === 'indexed' && result.completeness !== 'exact'
        && !result.candidates?.length && (result.indexing?.phase === 'scanning' || result.indexing?.phase === 'indexing')) {
        if (cancellation.token.isCancellationRequested || document.isClosed || document.version !== version
          || epoch !== this.epoch || definitionRequest !== this.definitionRequest
          || (result.version !== undefined && result.version !== version)) return undefined;
        continuing = true;
        this.status.text = '$(sync~spin) F#: indexing… (cancel)';
        this.status.tooltip = 'Looking for possible definitions for this request for up to 3 seconds. Click to cancel; Escape cancels while the editor owns the navigation operation.';
        this.status.command = 'fsharpNavigator.cancelDefinition';
        if (traceEnabled) trace('definitionContinuation', { requestId: definitionRequest, phase: result.indexing.phase });
        await pause(Math.min(200, Math.max(50, result.indexing.retryAfterMs ?? 100)), cancellation.token);
        if (cancellation.token.isCancellationRequested || document.isClosed || document.version !== version
          || epoch !== this.epoch || definitionRequest !== this.definitionRequest) return undefined;
        result = await cancellable(session.backend.request<QueryResult>(method, params, cancellation.token, 30_000, definitionRequest), cancellation.token);
      }
      if (cancellation.token.isCancellationRequested || document.isClosed || document.version !== version || epoch !== this.epoch || (result.version !== undefined && result.version !== version)) return undefined;
      if (definitionRequest !== undefined && definitionRequest !== this.definitionRequest) return undefined;
      if (referenceRequest !== undefined && (referenceRequest !== this.referenceRequest || referenceContext !== this.referenceContext)) {
        // Notification ownership must not suppress an independently valid exact
        // reference response. Existing token/version/epoch guards already ran.
        return result.completeness === 'exact' ? result : undefined;
      }
      if (traceEnabled) trace('providerResult', { requestId: definitionRequest, method, completeness: result.completeness, locations: result.locations?.length ?? 0, candidates: result.candidates?.length ?? 0 });
      this.lastResult = result;
      if (result.completeness !== 'exact') {
        const explanation = result.reason || result.diagnostics?.join('; ') || 'This binding is not supported by the current engine.';
        const details = result.candidateTruncated
          ? `${explanation} Candidate limit reached: ${result.candidateTruncationReason || 'additional possible targets may be omitted.'}`
          : explanation;
        this.status.text = method === 'navigation/references' ? 'F#: references incomplete' : 'F#: incomplete';
        this.status.tooltip = method === 'navigation/references'
          ? `${incompleteReferencesMessage} ${details}` : details;
        this.output.appendLine(`${method}: ${result.completeness} (${result.source ?? 'unknown'}): ${details}`);
        if (method === 'navigation/definition' && result.candidates?.length) {
          await cancellable(this.candidates.show(result.candidates, details, definitionRequest, async (target, pickerToken) => {
            const selectionCancellation = new vscode.CancellationTokenSource();
            const pickerCancellation = pickerToken?.onCancellationRequested(() => selectionCancellation.cancel());
            if (pickerToken?.isCancellationRequested) selectionCancellation.cancel();
            const current = () => (!followsCaret || (vscode.window.activeTextEditor === entryEditor
              && entryEditor.selections.length === 1 && entryEditor.selection.active.isEqual(position)))
              && !selectionCancellation.token.isCancellationRequested && !document.isClosed
              && document.version === version && epoch === this.epoch && definitionRequest === this.definitionRequest;
            this.navigation.add(selectionCancellation);
            try {
              if (!current()) return false;
              await this.synchronize(session, document);
              if (!current()) return false;
              if (traceEnabled) trace('definitionSelectionValidation', { requestId: definitionRequest });
              const fresh = await session.backend.request<QueryResult>('navigation/definition', {
                path: document.uri.fsPath, line: position.line, character: position.character,
                version, projectPath: this.selectedProjects.get(document.uri.toString())?.projectPath,
                ...(demandTraceConfig ? { diagnosticWatchSlot: demandTraceConfig.watchSlot } : undefined),
              }, selectionCancellation.token, 30_000, definitionRequest);
              if (!current() || (fresh.version !== undefined && fresh.version !== version)) return false;
              const values = fresh.completeness === 'exact' ? fresh.locations : fresh.candidates;
              return !!values?.some(value => {
                const item = location(value);
                return item.uri.toString() === target.uri.toString()
                  && item.range.start.line === target.range.start.line && item.range.start.character === target.range.start.character
                  && item.range.end.line === target.range.end.line && item.range.end.character === target.range.end.character;
              });
            } catch (error) {
              if (!(error instanceof vscode.CancellationError)) this.report(error);
              return false;
            } finally {
              pickerCancellation?.dispose();
              this.navigation.delete(selectionCancellation);
              selectionCancellation.dispose();
            }
          }, result), cancellation.token);
          if (definitionRequest === this.definitionRequest && (cancellation.token.isCancellationRequested || epoch !== this.epoch)) this.candidates.clear();
          return undefined;
        }
        if (referenceRequest !== undefined) {
          this.offerPossibleReferences(document, position, entryEditor, version, epoch, referenceContext, referenceRequest, token, details);
        } else if (method !== 'navigation/hover' && Date.now() - this.lastWarning > 3000) {
          this.lastWarning = Date.now();
          const warning = method === 'navigation/references'
            ? `F# Navigator could not verify a complete reference set; this does not mean there are no references. ${details}`
            : `F# Navigator could not verify a complete result. ${details}`;
          void vscode.window.showWarningMessage(warning, 'Show Details').then(action => {
            if (action) this.output.show(true);
          });
        }
        return undefined;
      }
      if (referenceRequest !== undefined) this.invalidateReferenceNotice();
      this.status.text = `F#: ${mode(folder.uri)}`;
      this.status.tooltip = `Verified by ${result.source ?? mode(folder.uri)}. Compiler analysis is never started automatically.`;
      return result;
    } catch (error) {
      if (!(error instanceof vscode.CancellationError)) this.report(error);
      return undefined;
    } finally {
      if (deadline) clearTimeout(deadline);
      caretChanged?.dispose(); editorChanged?.dispose();
      if (definitionRequest !== undefined && definitionRequest === this.definitionRequest) {
        this.activeDefinition = undefined;
        this.status.command = 'fsharpNavigator.selectMode';
        if (cancellation.token.isCancellationRequested) this.candidates.clear();
        if (continuing || timedOut) {
          if (timedOut) {
            this.status.text = 'F#: indexing budget reached';
            this.status.tooltip = 'This navigation stopped after 3 seconds. No further retries will run for it. Indexing may continue in the background; invoke definition again when ready.';
          } else if (cancellation.token.isCancellationRequested) {
            this.status.text = 'F#: navigation cancelled';
            this.status.tooltip = 'No further retries will run for the cancelled request.';
          }
        }
      }
      if (traceEnabled) trace('providerFinished', { requestId: definitionRequest, method, cancelled: cancellation.token.isCancellationRequested });
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
  constructor(private readonly getNavigator: () => Navigator) {}
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
    const navigator = this.getNavigator();
    const nodes: TreeNode[] = [];
    for (const folder of vscode.workspace.workspaceFolders ?? []) {
      try {
        const session = await navigator.session(folder);
        const result = await session.backend.request<TreeResult | TreeNode[]>('workspace/tree');
        const children = Array.isArray(result) ? result : result.nodes;
        const attachContext = (node: TreeNode, projectPath?: string): TreeNode => {
          const owningProject = node.kind === 'project' ? node.path : projectPath;
          return { ...node, projectPath: owningProject, workspaceUri: folder.uri.toString(), children: node.children?.map(child => attachContext(child, owningProject)) };
        };
        for (const child of children ?? []) nodes.push(attachContext(child));
        if (!Array.isArray(result) && result.diagnostics?.length) navigator.output.appendLine(result.diagnostics.join('\n'));
      } catch (error) { navigator.report(error); }
    }
    return nodes;
  }
  dispose(): void { this.emitter.dispose(); }
}

export function activate(context: vscode.ExtensionContext) {
  if (traceEnabled) trace('activationStart');
  let navigator: Navigator | undefined;
  const candidates = new DefinitionCandidates();
  const possibleReferences = new PossibleReferences();
  // Command activation may be requested for another language. Only F# providers, views
  // and commands should construct the status item, file watchers and backend sessions.
  const getNavigator = (): Navigator => {
    if (!navigator) {
      if (traceEnabled) trace('navigatorStart');
      navigator = new Navigator(context, () => tree.refresh(), candidates, possibleReferences);
      if (traceEnabled) trace('navigatorReady');
    }
    return navigator;
  };
  const tree = new SolutionTree(getNavigator);
  const metadata = new MetadataDocuments();
  const selector: vscode.DocumentSelector = { language: 'fsharp', scheme: 'file' };
  const definitionProvider: vscode.DefinitionProvider = {
      async provideDefinition(document, position, token) {
        const result = await getNavigator().query('navigation/definition', document, position, token);
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
        } catch (error) { getNavigator().report(error); return undefined; }
      }
    };
  context.subscriptions.push({ dispose: () => navigator?.dispose() }, tree, metadata, candidates, possibleReferences,
    vscode.commands.registerCommand('fsharpNavigator.showPossibleReferences', () => getNavigator().showPossibleReferences()),
    vscode.commands.registerCommand('fsharpNavigator.openPossibleReference', (id: string) => possibleReferences.open(id)),
    vscode.commands.registerCommand('fsharpNavigator.chooseCandidates', (generation?: unknown) => candidates.choose(generation)),
    vscode.commands.registerCommand('fsharpNavigator.cancelDefinition', () => navigator?.cancelDefinition()),
    vscode.workspace.registerTextDocumentContentProvider(metadataScheme, metadata),
    vscode.window.registerTreeDataProvider('fsharpNavigator.solution', tree),
    vscode.languages.registerDefinitionProvider(selector, definitionProvider),
    vscode.languages.registerReferenceProvider(selector, {
      async provideReferences(document, position, referenceContext, token) {
        const result = await getNavigator().query('navigation/references', document, position, token, referenceContext.includeDeclaration);
        return result?.locations?.map(location);
      }
    }),
    vscode.languages.registerHoverProvider(selector, {
      async provideHover(document, position, token) {
        const result = await getNavigator().query('navigation/hover', document, position, token);
        if (!result?.hover || (result.ownership !== 'external' && result.hover.isExternal !== true)) return undefined;
        const markdown = new vscode.MarkdownString(result.hover.markdown);
        markdown.isTrusted = false;
        markdown.supportHtml = false;
        return new vscode.Hover(markdown, result.hover.range ? range(result.hover.range) : undefined);
      }
    }),
    vscode.commands.registerCommand('fsharpNavigator.openProjectFile', (uri: vscode.Uri, projectPath: string, workspaceUri: string) => getNavigator().openProjectFile(uri, projectPath, workspaceUri)),
    vscode.commands.registerCommand('fsharpNavigator.refresh', () => tree.refresh()),
    vscode.commands.registerCommand('fsharpNavigator.indexWorkspace', () => getNavigator().indexWorkspace()),
    vscode.commands.registerCommand('fsharpNavigator.restart', () => getNavigator().restart()),
    vscode.commands.registerCommand('fsharpNavigator.selectMode', async () => {
      const choice = await vscode.window.showQuickPick([
        { label: 'Indexed', description: 'Rust index; no automatic compiler analysis', value: 'indexed' },
        { label: 'Lexical', description: 'Rust syntax and scope analysis', value: 'lexical' },
        { label: 'Semantic (experimental)', description: 'Explicit .NET compiler helper; potentially expensive', value: 'semantic' },
      ], { title: 'F# Navigator mode' });
      if (choice) await vscode.workspace.getConfiguration('fsharpNavigator').update('mode', choice.value, vscode.ConfigurationTarget.Workspace);
    })
  );
  if (traceEnabled) trace('activationEnd');
  // Export observable state for the real extension-host tests, not an alternate analysis implementation.
  return {
    tree,
    candidates,
    definitionProvider,
    possibleReferences,
    get lastPossibleReferences() { return navigator?.lastPossibleReferences; },
    get traceEvents() { return traceEvents(); },
    async stats() {
      const results: unknown[] = [];
      for (const session of navigator?.sessions.values() ?? []) results.push(await session.backend.request('workspace/stats'));
      return results;
    },
    get lastResult() { return navigator?.lastResult; },
    get processIds() { return Array.from(navigator?.sessions.values() ?? [], session => session.backend.pid); },
    restart() { getNavigator().restart(); },
  };
}

export function deactivate(): void { /* Extension subscriptions own process disposal. */ }
