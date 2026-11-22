export interface OverlayDocument {
  readonly version: number;
  readonly isClosed: boolean;
  getText(): string;
}

type Send = (method: string, params: unknown) => Promise<unknown>;

/** At most one in-flight text snapshot and one latest document reference/path. */
export class OverlayQueue {
  private readonly pending = new Map<string, OverlayDocument | null>();
  // A lifecycle boundary must survive coalescing to a newer document reference.
  private readonly pendingCloses = new Set<string>();
  private readonly acknowledged = new Map<string, number>();
  private running: Promise<void> | undefined;
  private disposed = false;
  private activePath: string | undefined;
  private failure: unknown;

  constructor(private readonly ready: Promise<unknown>, private readonly send: Send, private readonly onError: (error: unknown) => void) {}

  schedule(path: string, document: OverlayDocument): void {
    if (this.disposed) return;
    if (!this.pending.has(path) && this.activePath !== path && this.acknowledged.get(path) === document.version) return;
    // Do not call getText here: an edit storm stores only the latest live document.
    this.pending.set(path, document);
    this.start();
  }

  close(path: string): void {
    if (this.disposed) return;
    if (!this.pending.has(path) && !this.acknowledged.has(path) && this.activePath !== path) return;
    this.pendingCloses.add(path);
    this.pending.set(path, null);
    this.start();
  }

  async flush(): Promise<void> {
    if (!this.running && this.pending.size) this.start();
    while (this.running) await this.running;
    if (this.failure) throw this.failure;
  }

  private start(): void {
    if (this.running || this.disposed) return;
    this.failure = undefined;
    this.running = this.drain().finally(() => {
      this.running = undefined;
      if (this.pending.size && !this.disposed && !this.failure) this.start();
    });
    // One handler per drain, not a promise closure retaining each keystroke's text.
    void this.running.catch(error => { if (!this.disposed) this.onError(error); });
  }

  private async drain(): Promise<void> {
    let active: [string, OverlayDocument | null] | undefined;
    try {
      await this.ready;
      while (!this.disposed && this.pending.size) {
        const [path, document] = this.pending.entries().next().value!;
        active = [path, document];
        this.pending.delete(path);
        this.activePath = path;
        if (this.pendingCloses.has(path) || !document || document.isClosed) {
          this.pendingCloses.add(path);
          await this.send('document/close', { path });
          if (this.disposed) return;
          this.pendingCloses.delete(path);
          this.acknowledged.delete(path);
          // A close ACK covers all earlier lifetimes. Prefer any newer pending
          // state; otherwise reopen the document retained alongside the barrier.
          if (document && !document.isClosed && !this.pending.has(path)) this.pending.set(path, document);
          this.activePath = undefined;
          active = undefined;
          continue;
        }
        const version = document.version;
        const text = document.getText();
        if (Buffer.byteLength(text, 'utf8') > 8 * 1024 * 1024) throw new Error('F# Navigator cannot synchronize a document larger than 8 MiB. No current result is available.');
        await this.send(this.acknowledged.has(path) ? 'document/change' : 'document/open', { path, text, version });
        if (this.disposed) return;
        this.acknowledged.set(path, version);
        this.activePath = undefined;
        active = undefined;
        if (!this.pendingCloses.has(path) && this.pending.get(path) === document && document.version === version) this.pending.delete(path);
      }
    } catch (error) {
      this.activePath = undefined;
      // Retain failed work for an explicit retry. A failed close must not leave
      // an old overlay silently active while a later query reports success.
      if (!this.disposed && active && !this.pending.has(active[0])) this.pending.set(...active);
      this.failure = error;
      throw error;
    }
  }

  dispose(): void { this.disposed = true; this.pending.clear(); this.pendingCloses.clear(); this.acknowledged.clear(); }
}
