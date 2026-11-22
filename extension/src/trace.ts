/** Opt-in, bounded timing evidence. No source text, paths or symbol names. */
export const traceEnabled = process.env.FSHARP_NAVIGATOR_TRACE === '1';
interface TraceEvent {
  event: string;
  monotonicNs: string;
  details?: Record<string, string | number | boolean | undefined>;
}
const events: TraceEvent[] = [];
export function trace(event: string, details?: TraceEvent['details']): void {
  if (traceEnabled && events.length < 2048) {
    events.push({ event, monotonicNs: process.hrtime.bigint().toString(), details });
  }
}
export function traceEvents(): TraceEvent[] { return events.slice(); }
