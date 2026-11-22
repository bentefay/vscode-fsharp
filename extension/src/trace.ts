/** Opt-in, bounded timing evidence. No source text, paths or symbol names. */
export const traceEnabled = process.env.FSHARP_NAVIGATOR_TRACE === '1';
interface DemandTraceConfig { modelOrdinals: number[]; watchSlot: number }
/** Test-only numeric selectors. They never carry paths or change navigation routing. */
export function parseDemandTraceConfig(ordinals: string | undefined, slot = '0'): DemandTraceConfig | undefined {
  if (!ordinals || ordinals.length > 64 || !/^[0-3]$/.test(slot)) return undefined;
  try {
    const values: unknown = JSON.parse(ordinals);
    if (!Array.isArray(values) || values.length < 1 || values.length > 4
      || !values.every(value => Number.isSafeInteger(value) && value >= 0 && value < 100_000)
      || new Set(values).size !== values.length || Number(slot) >= values.length) return undefined;
    return { modelOrdinals: values, watchSlot: Number(slot) };
  } catch { return undefined; }
}
export const demandTraceConfig = traceEnabled ? parseDemandTraceConfig(
  process.env.FSHARP_NAVIGATOR_TRACE_MODEL_ORDINALS, process.env.FSHARP_NAVIGATOR_TRACE_WATCH_SLOT,
) : undefined;

/** Whitelist bounded diagnostic primitives; never copy an arbitrary backend object into traces. */
export function demandSwitchTraceFields(value: unknown): Record<string, string | number | boolean> {
  const fields: Record<string, string | number | boolean> = {};
  if (!value || typeof value !== 'object') return fields;
  const data = value as Record<string, unknown>;
  for (const key of ['targetSlot', 'targetModelOrdinal', 'beforeGeneration', 'afterGeneration',
    'beforeOrderSerial', 'afterOrderSerial', 'beforeOrderOriginSlot', 'afterOrderOriginSlot', 'beforeCursor', 'afterCursor',
    'beforeTargetOrderOrdinal', 'afterTargetOrderOrdinal', 'originSlot',
    'seedAttempts', 'seedCompleted', 'seedMicros', 'lastCompletedSliceId',
    'lastCompletedSliceMicros', 'lastCompletedSliceEndBeforeDispatchMicros',
    'sliceBeforeGeneration', 'sliceAfterGeneration', 'sliceBeforeOrderSerial',
    'sliceAfterOrderSerial', 'sliceBeforeCursor', 'sliceAfterCursor']) {
    const number = data[key];
    if (typeof number === 'number' && Number.isFinite(number) && number >= 0 && number <= Number.MAX_SAFE_INTEGER)
      fields[`demand_${key}`] = number;
  }
  for (const key of ['targetSelectedByInitialCandidateLookup', 'candidateIndicesTruncated',
    'identityVerified', 'returnedIndexingHint', 'targetSeedCompleted', 'admissionInsideLastCompletedSlice']) {
    if (typeof data[key] === 'boolean') fields[`demand_${key}`] = data[key];
  }
  const enums: Record<string, readonly string[]> = {
    beforeTargetState: ['unconstructed', 'pending', 'consumed', 'absent'],
    afterTargetState: ['unconstructed', 'pending', 'consumed', 'absent'],
    beforePhase: ['idle', 'scanning', 'indexing', 'ready', 'limited'],
    afterPhase: ['idle', 'scanning', 'indexing', 'ready', 'limited'],
    postingStatus: ['unknown'], candidateIndexLane: ['result.candidates'],
  };
  for (const [key, allowed] of Object.entries(enums)) {
    if (typeof data[key] === 'string' && allowed.includes(data[key])) fields[`demand_${key}`] = data[key];
  }
  if (Array.isArray(data.candidateIndices)) {
    for (let i = 0; i < Math.min(4, data.candidateIndices.length); i++) {
      const index: unknown = data.candidateIndices[i];
      if (typeof index === 'number' && Number.isSafeInteger(index) && index >= 0 && index < 200)
        fields[`demand_candidateIndex${i}`] = index;
    }
  }
  return fields;
}
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
