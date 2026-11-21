import * as vscode from 'vscode';

export const metadataScheme = 'fsharp-navigator';
export interface VirtualDocument { uri: string; language: 'fsharp' | 'csharp'; text: string }

/** The helper's older protocol uses `path` for both paths and metadata URIs. */
export function locationUri(value: { uri?: string; path?: string }): vscode.Uri {
  if (value.uri) return vscode.Uri.parse(value.uri);
  if (!value.path) throw new Error('Engine returned a location without a URI or path.');
  return value.path.startsWith(`${metadataScheme}:`) ? vscode.Uri.parse(value.path) : vscode.Uri.file(value.path);
}

/** Read-only content provider; generated external declarations and implementations enter this cache. */
export class MetadataDocuments implements vscode.TextDocumentContentProvider, vscode.Disposable {
  private readonly entries = new Map<string, { text: string; bytes: number }>();
  private readonly changed = new vscode.EventEmitter<vscode.Uri>();
  readonly onDidChange = this.changed.event;
  private bytes = 0;

  constructor(
    private readonly maxBytes = 2 * 1024 * 1024,
    private readonly maxDocuments = 64,
    private readonly maxDocumentBytes = 256 * 1024,
  ) {}

  private key(uri: vscode.Uri): string {
    if (uri.scheme !== metadataScheme || uri.authority !== 'metadata' || !(uri.path.endsWith('.fsi') || uri.path.endsWith('.cs')) || uri.query || uri.fragment) {
      throw new Error('Engine returned an unsupported external-signature URI.');
    }
    return uri.toString();
  }

  accept(documents: VirtualDocument[] = []): void {
    // Validate a complete response before changing cache state. UTF-16 units
    // account for JS string storage; source text itself stays in the Rust engine.
    const incoming = new Map<string, { uri: vscode.Uri; text: string; bytes: number }>();
    for (const document of documents) {
      if (typeof document.uri !== 'string' || typeof document.text !== 'string' || !['fsharp', 'csharp'].includes(document.language)) {
        throw new Error('Engine returned an invalid external-signature document.');
      }
      const uri = vscode.Uri.parse(document.uri);
      const key = this.key(uri);
      if ((document.language === 'fsharp') !== uri.path.endsWith('.fsi')) throw new Error('External document language does not match its URI suffix.');
      const bytes = 2 * (key.length + document.text.length);
      if (bytes > this.maxDocumentBytes) throw new Error('External signature exceeded the document cache limit.');
      incoming.set(key, { uri, text: document.text, bytes });
    }
    let incomingBytes = 0;
    for (const entry of incoming.values()) incomingBytes += entry.bytes;
    if (incomingBytes > this.maxBytes || incoming.size > this.maxDocuments) throw new Error('External signatures exceeded the document cache limit.');
    const refresh: vscode.Uri[] = [];
    for (const [key, entry] of incoming) {
      const previous = this.entries.get(key);
      if (previous) {
        this.entries.delete(key);
        this.bytes -= previous.bytes;
        if (previous.text !== entry.text) refresh.push(entry.uri);
      }
    }
    while (this.bytes + incomingBytes > this.maxBytes || this.entries.size + incoming.size > this.maxDocuments) {
      const oldest = this.entries.keys().next().value;
      if (oldest === undefined) break;
      this.bytes -= this.entries.get(oldest)!.bytes;
      this.entries.delete(oldest);
    }
    for (const [key, entry] of incoming) {
      this.entries.set(key, { text: entry.text, bytes: entry.bytes });
      this.bytes += entry.bytes;
    }
    for (const uri of refresh) this.changed.fire(uri);
  }

  has(uri: vscode.Uri): boolean { return this.entries.has(this.key(uri)); }

  provideTextDocumentContent(uri: vscode.Uri): string {
    const key = this.key(uri);
    const entry = this.entries.get(key);
    if (!entry) throw new Error('This external signature expired from the bounded cache. Use Go to Definition again to reload it.');
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.text;
  }

  dispose(): void {
    this.entries.clear();
    this.bytes = 0;
    this.changed.dispose();
  }
}
