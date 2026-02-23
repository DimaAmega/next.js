import type { Project } from '../../../build/swc/types'
import * as Log from '../../../build/output/log'
import type { Span } from '../../../trace'

const MILLISECONDS_IN_NANOSECOND = BigInt(1_000_000)

function msToNs(ms: number): bigint {
  return BigInt(Math.floor(ms)) * MILLISECONDS_IN_NANOSECOND
}

/**
 * Subscribes to compilation events for `project` and prints them using the
 * `Log` library.
 *
 * When `parentSpan` is provided, `PersistenceEvent` and `CompactionEvent`
 * events are also recorded as trace spans in the `.next/trace` file.
 *
 * The `signal` argument is partially implemented. The abort may not happen until the next
 * compilation event arrives.
 */
export function backgroundLogCompilationEvents(
  project: Project,
  {
    eventTypes,
    signal,
    parentSpan,
  }: { eventTypes?: string[]; signal?: AbortSignal; parentSpan?: Span } = {}
) {
  ;(async function () {
    for await (const event of project.compilationEventsSubscribe(eventTypes)) {
      if (signal?.aborted) {
        return
      }

      // Record persistence and compaction events as trace spans
      if (parentSpan && event.eventJson) {
        if (event.typeName === 'PersistenceEvent') {
          try {
            const data = JSON.parse(event.eventJson)
            parentSpan.manualTraceChild(
              'turbopack-persistence',
              msToNs(data.start_time_ms),
              msToNs(data.end_time_ms),
              {
                reason: data.reason,
                snapshotDurationMs: data.snapshot_duration_ms,
                persistDurationMs: data.persist_duration_ms,
                taskCount: data.task_count,
              }
            )
          } catch {}
        } else if (event.typeName === 'CompactionEvent') {
          try {
            const data = JSON.parse(event.eventJson)
            parentSpan.manualTraceChild(
              'turbopack-compaction',
              msToNs(data.start_time_ms),
              msToNs(data.end_time_ms),
              {
                durationMs: data.duration_ms,
              }
            )
          } catch {}
        }
      }

      switch (event.severity) {
        case 'EVENT':
          Log.event(event.message)
          break
        case 'TRACE':
          Log.trace(event.message)
          break
        case 'INFO':
          Log.info(event.message)
          break
        case 'WARNING':
          Log.warn(event.message)
          break
        case 'ERROR':
          Log.error(event.message)
          break
        case 'FATAL':
          Log.error(event.message)
          break
        default:
          break
      }
    }
  })()
}
