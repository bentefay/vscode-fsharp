import * as vscode from 'vscode';
import * as path from 'node:path';
import { locationUri } from './virtualDocuments';

interface Span { start: { line: number; character: number }; end: { line: number; character: number } }
interface Occurrence { uri?: string; path?: string; range: Span }
export interface SourceIdentity { uri: string; sourceHash: string; version: number | null }
export interface OccurrenceResult {
  locations: Occurrence[];
  candidates?: Occurrence[];
  completeness: string;
  candidateKind: string;
  candidateCompleteness: string;
  candidateTruncated?: boolean;
  candidateTruncationReason?: string;
  queryName?: string | null;
  version?: number | null;
  occurrenceSearch: { scannedFiles: number; skippedFiles?: number; maxSourceBytes?: number; maxTotalBytes?: number; maxFiles: number; maxRows: number; maxLocations: number; preparationPending?: boolean; discoveryPhase?: string; discoveryComplete?: boolean };
  occurrenceIdentity: { origin: SourceIdentity; targets: SourceIdentity[] };
}
export interface OccurrenceRow { id: string; location: vscode.Location; identity: SourceIdentity }
export function sameIdentity(a: SourceIdentity | undefined, b: SourceIdentity | undefined): boolean {
  return !!a && !!b && !!a.sourceHash && a.uri === b.uri && a.sourceHash === b.sourceHash && a.version === b.version;
}
export function occurrenceLocation(value: Occurrence): vscode.Location {
  const r = value.range;
  return new vscode.Location(locationUri(value), new vscode.Range(r.start.line, r.start.character, r.end.line, r.end.character));
}
export function sameLocation(a: vscode.Location, b: vscode.Location): boolean {
  return a.uri.toString() === b.uri.toString() && a.range.start.isEqual(b.range.start) && a.range.end.isEqual(b.range.end);
}
export function validOccurrenceResult(result: OccurrenceResult): boolean {
  return result.completeness === 'unavailable' && result.locations?.length === 0
    && result.candidateKind === 'same-name-occurrences' && result.candidateCompleteness === 'incomplete'
    && !!result.occurrenceIdentity?.origin?.sourceHash && Array.isArray(result.occurrenceIdentity.targets)
    && !!result.occurrenceSearch && Number.isFinite(result.occurrenceSearch.scannedFiles);
}

/** Explicit occurrence results never enter the registered reference provider. */
export class PossibleReferences implements vscode.TreeDataProvider<OccurrenceRow>, vscode.Disposable {
  private readonly emitter = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.emitter.event;
  private view: vscode.TreeView<OccurrenceRow> | undefined;
  private rows: OccurrenceRow[] = [];
  private generation = 0;
  private selection = 0;
  private validate: ((row: OccurrenceRow) => Promise<boolean>) | undefined;
  getChildren(): OccurrenceRow[] { return this.rows; }
  getParent(): undefined { return undefined; }
  get visible(): boolean { return this.view?.visible ?? false; }
  getTreeItem(row: OccurrenceRow): vscode.TreeItem {
    if (!this.rows.includes(row)) throw new Error('Possible-reference row is no longer current.');
    const item = new vscode.TreeItem(`${path.basename(row.location.uri.fsPath)}:${row.location.range.start.line + 1}:${row.location.range.start.character + 1}`);
    item.id = row.id;
    item.description = row.location.uri.fsPath;
    item.tooltip = `Same-name occurrence; binding unverified. May be a declaration, inactive branch or unrelated symbol.\n${row.location.uri.fsPath}`;
    item.command = { command: 'fsharpNavigator.openPossibleReference', title: 'Open Same-name Occurrence', arguments: [row.id] };
    return item;
  }
  clear(message = 'Results invalidated. Run Show Possible References again. No complete reference set has been established.'): void {
    this.generation++; this.rows = []; this.validate = undefined;
    if (this.view) { this.view.message = message; this.emitter.fire(); }
  }
  async show(result: OccurrenceResult, validate: (row: OccurrenceRow) => Promise<boolean>): Promise<void> {
    if (!validOccurrenceResult(result)) throw new Error('Invalid possible-reference response; no occurrence list was accepted.');
    const generation = ++this.generation;
    this.rows = (result.candidates ?? []).map((value, index) => {
      const location = occurrenceLocation(value);
      const identity = result.occurrenceIdentity.targets.find(target => target.uri === location.uri.toString());
      if (!identity?.sourceHash) throw new Error('Possible-reference target identity is unavailable.');
      return { id: `occurrence-${generation}-${index}`, location, identity };
    });
    this.validate = validate;
    this.view ??= vscode.window.createTreeView('fsharpNavigator.possibleReferences', { treeDataProvider: this, showCollapseAll: false });
    this.view.title = 'Possible references (incomplete)';
    this.view.description = result.queryName ? `${result.queryName} · ${path.basename(vscode.Uri.parse(result.occurrenceIdentity.origin.uri).fsPath)}` : 'No identifier at cursor';
    this.emitter.fire();
    const scope = result.occurrenceSearch;
    this.view.message = `${result.queryName ? `Same-name occurrences for ${JSON.stringify(result.queryName)} only; binding unverified.` : 'No identifier at the requested cursor; no occurrence query name was available.'} May include declarations, inactive branches and unrelated symbols. Showing ${this.rows.length} matches from ${scope.scannedFiles} scanned files (up to ${scope.maxFiles} files and ${scope.maxLocations} matches). ${scope.skippedFiles ?? 0} files skipped; data and inspected-occurrence limits also apply. Not a complete reference set, even when empty. Discovery ${scope.discoveryPhase ?? 'unknown'}; ${scope.discoveryComplete ? 'discovery finished, but search coverage remains limited' : 'workspace coverage is not known complete'}.${scope.preparationPending ? ' Workspace preparation is pending; only currently available sources were searched.' : ''}${result.candidateTruncated ? ' Search stopped before all potential matches were inspected; more occurrences may exist.' : ''}`;
    if (this.rows.length) await this.view.reveal(this.rows[0], { select: false, focus: false });
    else await vscode.commands.executeCommand('fsharpNavigator.possibleReferences.focus');
  }
  async open(id: string): Promise<void> {
    const row = this.rows.find(item => item.id === id), generation = this.generation;
    if (!row || !this.validate) return;
    const selection = ++this.selection;
    const valid = await this.validate(row);
    if (generation !== this.generation || selection !== this.selection) return;
    if (!valid) { this.clear(); return; }
    await vscode.commands.executeCommand('vscode.open', row.location.uri, { selection: row.location.range, preview: false });
  }
  dispose(): void { this.clear(); this.view?.dispose(); this.emitter.dispose(); }
}
