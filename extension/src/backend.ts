import { spawn, ChildProcessWithoutNullStreams } from 'node:child_process';
import * as vscode from 'vscode';
import { trace, traceEnabled, demandSwitchTraceFields } from './trace';

interface DocumentUpdateTrace {
  documentKey?: string; version?: number; sourceHash?: string; bytes?: number; changed?: boolean;
  validationMicros?: number; indexUpdateMicros?: number; hashMicros?: number; nameScanMicros?: number;
  lookupMicros?: number; transactionBeginMicros?: number; postingsDeleteMicros?: number; namesInsertMicros?: number; stateWriteMicros?: number; commitMicros?: number;
  namesCount?: number; overlayStoreMicros?: number; proofCacheEvicted?: number; snapshotMicros?: number; totalMicros?: number;
}
interface Pending {
  method: string;
  providerRequestId?: number;
  resolve(value: unknown): void;
  reject(reason: Error): void;
  timer: ReturnType<typeof setTimeout>;
  cancellation?: vscode.Disposable;
}
interface Outbound { id?: number; line: string; bytes: number }
export interface TransportLimits { maxPendingRequests?: number; maxQueuedBytes?: number; maxMessageBytes?: number }

/** JSON-lines transport only. All source analysis belongs to the Rust engine. */
export class Backend implements vscode.Disposable {
  private readonly process: ChildProcessWithoutNullStreams;
  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private buffer = '';
  private disposed = false;
  private stderr = '';
  private readonly outbound: Outbound[] = [];
  private queuedBytes = 0;
  private blocked = false;
  private readonly maxPendingRequests: number;
  private readonly maxQueuedBytes: number;
  private readonly maxMessageBytes: number;
  readonly startedAt = Date.now();

  constructor(binary: string, private readonly output: vscode.OutputChannel, limits: TransportLimits = {}) {
    this.maxPendingRequests = limits.maxPendingRequests ?? 64;
    this.maxQueuedBytes = limits.maxQueuedBytes ?? 16 * 1024 * 1024;
    this.maxMessageBytes = limits.maxMessageBytes ?? 12 * 1024 * 1024;
    if (traceEnabled) trace('nativeSpawnStart');
    this.process = spawn(binary, ['daemon'], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    if (traceEnabled) trace('nativeSpawn', { pid: this.process.pid });
    this.process.stdout.setEncoding('utf8');
    this.process.stderr.setEncoding('utf8');
    this.process.stdin.on('drain', () => { this.blocked = false; this.pump(); });
    this.process.stdin.on('error', error => this.fail(error));
    this.process.stdout.on('data', (chunk: string) => this.read(chunk));
    this.process.stderr.on('data', (chunk: string) => {
      this.stderr = (this.stderr + chunk).slice(-8192);
      output.append(chunk);
    });
    this.process.on('error', error => { this.disposed = true; this.fail(error); });
    this.process.on('exit', (code, signal) => {
      this.disposed = true;
      this.fail(new Error(`F# Navigator engine exited (${signal ?? code}). ${this.stderr.trim()}`));
    });
  }

  get pid(): number | undefined { return this.process.pid; }
  get closed(): boolean { return this.disposed; }

  request<T>(method: string, params: unknown = {}, token?: vscode.CancellationToken, timeoutMs = 30_000, providerRequestId?: number): Promise<T> {
    if (this.disposed) return Promise.reject(new Error('F# Navigator engine is not running.'));
    if (token?.isCancellationRequested) return Promise.reject(new vscode.CancellationError());
    if (this.pending.size >= this.maxPendingRequests) return Promise.reject(new Error('F# Navigator request queue is full; no complete result is available.'));
    const id = this.nextId++;
    const correlatedProvider = traceEnabled && typeof providerRequestId === 'number'
      && Number.isSafeInteger(providerRequestId) && providerRequestId > 0 ? providerRequestId : undefined;
    if (traceEnabled) trace('transportRequest', { id, method, pid: this.process.pid, providerRequestId: correlatedProvider });
    return new Promise<T>((resolve, reject) => {
      const finishWithError = (error: Error) => {
        const entry = this.pending.get(id);
        if (!entry) return;
        this.pending.delete(id);
        clearTimeout(entry.timer);
        entry.cancellation?.dispose();
        const queued = this.outbound.findIndex(item => item.id === id);
        if (queued >= 0) {
          this.queuedBytes -= this.outbound[queued].bytes;
          this.outbound.splice(queued, 1);
        } else this.notify('$/cancelRequest', { id });
        reject(error);
      };
      const timer = setTimeout(() => finishWithError(new Error(`F# Navigator timed out during ${method}; no complete result is available.`)), timeoutMs);
      const entry: Pending = { method, resolve: value => resolve(value as T), reject, timer };
      if (correlatedProvider !== undefined) entry.providerRequestId = correlatedProvider;
      this.pending.set(id, entry);
      entry.cancellation = token?.onCancellationRequested(() => finishWithError(new vscode.CancellationError()));
      try { this.enqueue({ id, method, params }, id); }
      catch (error) { finishWithError(error instanceof Error ? error : new Error(String(error))); }
    });
  }

  notify(method: string, params: unknown): void {
    if (!this.disposed && this.process.stdin.writable) {
      // Notifications must not create an unbounded second queue during pressure.
      try { this.enqueue({ method, params }); } catch { /* Cancellation is advisory; stale replies are discarded. */ }
    }
  }

  private enqueue(message: unknown, id?: number): void {
    const line = JSON.stringify(message) + '\n';
    const bytes = Buffer.byteLength(line, 'utf8');
    if (bytes > this.maxMessageBytes || this.queuedBytes + this.process.stdin.writableLength + bytes > this.maxQueuedBytes) {
      throw new Error('F# Navigator transport byte budget exceeded; no complete result is available.');
    }
    this.outbound.push({ id, line, bytes });
    this.queuedBytes += bytes;
    this.pump();
  }

  private pump(): void {
    while (!this.disposed && !this.blocked && this.outbound.length) {
      const item = this.outbound.shift()!;
      this.queuedBytes -= item.bytes;
      if (item.id !== undefined && !this.pending.has(item.id)) continue;
      // write(false) means stop until drain; never keep feeding Node's internal buffer.
      if (traceEnabled && item.id !== undefined) trace('transportWrite', { pid: this.process.pid, id: item.id, method: this.pending.get(item.id)?.method });
      this.blocked = !this.process.stdin.write(item.line, error => { if (error) this.fail(error); });
    }
  }

  private read(chunk: string): void {
    this.buffer += chunk;
    if (this.buffer.length > 32 * 1024 * 1024) {
      this.fail(new Error('F# Navigator engine response exceeded the transport limit.'));
      this.dispose();
      return;
    }
    let end: number;
    while ((end = this.buffer.indexOf('\n')) !== -1) {
      const line = this.buffer.slice(0, end).trim();
      this.buffer = this.buffer.slice(end + 1);
      if (!line) continue;
      let message: { id?: number; result?: unknown; error?: { message?: string; code?: number | string }; nativeTiming?: { queueMs?: number; dispatchMs?: number }; demandSwitch?: unknown; documentUpdate?: DocumentUpdateTrace; localProofAttempt?: { attempt?: number; documentVersion?: number; outcome?: string; parseMicros?: number; totalMicros?: number; visitedNodes?: number; parsedBytes?: number } | null };
      try { message = JSON.parse(line); }
      catch {
        this.fail(new Error('F# Navigator engine emitted an invalid protocol response.'));
        this.dispose();
        return;
      }
      if (typeof message.id !== 'number') continue;
      const entry = this.pending.get(message.id);
      if (!entry) continue; // Cancelled or obsolete request.
      if (traceEnabled) trace('transportResponse', {
        pid: this.process.pid, id: message.id, method: entry.method, providerRequestId: entry.providerRequestId,
        queueMs: message.nativeTiming?.queueMs, dispatchMs: message.nativeTiming?.dispatchMs,
        localProofAttempt: message.localProofAttempt?.attempt,
        localProofVersion: message.localProofAttempt?.documentVersion,
        localProofOutcome: message.localProofAttempt?.outcome,
        localProofParseMicros: message.localProofAttempt?.parseMicros,
        localProofTotalMicros: message.localProofAttempt?.totalMicros,
        localProofVisitedNodes: message.localProofAttempt?.visitedNodes,
        localProofParsedBytes: message.localProofAttempt?.parsedBytes,
        updateDocumentKey: message.documentUpdate?.documentKey,
        updateVersion: message.documentUpdate?.version,
        updateSourceHash: message.documentUpdate?.sourceHash,
        updateBytes: message.documentUpdate?.bytes,
        updateChanged: message.documentUpdate?.changed,
        updateValidationMicros: message.documentUpdate?.validationMicros,
        updateIndexMicros: message.documentUpdate?.indexUpdateMicros,
        updateHashMicros: message.documentUpdate?.hashMicros,
        updateNameScanMicros: message.documentUpdate?.nameScanMicros,
        updateLookupMicros: message.documentUpdate?.lookupMicros,
        updateTransactionBeginMicros: message.documentUpdate?.transactionBeginMicros,
        updatePostingsDeleteMicros: message.documentUpdate?.postingsDeleteMicros,
        updateNamesInsertMicros: message.documentUpdate?.namesInsertMicros,
        updateStateWriteMicros: message.documentUpdate?.stateWriteMicros,
        updateCommitMicros: message.documentUpdate?.commitMicros,
        updateNamesCount: message.documentUpdate?.namesCount,
        updateOverlayStoreMicros: message.documentUpdate?.overlayStoreMicros,
        updateProofCacheEvicted: message.documentUpdate?.proofCacheEvicted,
        updateSnapshotMicros: message.documentUpdate?.snapshotMicros,
        updateTotalMicros: message.documentUpdate?.totalMicros,
        ...demandSwitchTraceFields(message.demandSwitch),
      });
      this.pending.delete(message.id);
      clearTimeout(entry.timer);
      entry.cancellation?.dispose();
      if (message.error) entry.reject(new Error(message.error.message ?? `Engine error ${message.error.code}`));
      else entry.resolve(message.result);
    }
  }

  private fail(error: Error): void {
    for (const entry of this.pending.values()) {
      clearTimeout(entry.timer);
      entry.cancellation?.dispose();
      entry.reject(error);
    }
    this.pending.clear();
    this.outbound.length = 0;
    this.queuedBytes = 0;
  }

  dispose(): void {
    if (this.disposed) return;
    this.outbound.length = 0;
    this.queuedBytes = 0;
    this.notify('shutdown', {});
    this.disposed = true;
    this.fail(new vscode.CancellationError());
    this.process.stdin.end();
    const timer = setTimeout(() => this.process.kill(), 500);
    timer.unref();
    this.process.once('exit', () => clearTimeout(timer));
  }
}
