import { useMemo, useState, useCallback, useEffect } from 'react'
import { CodeFrame } from '../../components/code-frame/code-frame'
import { ErrorOverlayCallStack } from '../../components/errors/error-overlay-call-stack/error-overlay-call-stack'
import { PSEUDO_HTML_DIFF_STYLES } from './component-stack-pseudo-html'
import { ErrorCause, styles as errorCauseStyles } from './error-cause'
import {
  useFrames,
  type ReadyRuntimeError,
} from '../../utils/get-error-by-type'

type RuntimeErrorProps = {
  error: ReadyRuntimeError
  dialogResizerRef: React.RefObject<HTMLDivElement | null>
}

export function RuntimeError({ error, dialogResizerRef }: RuntimeErrorProps) {
  const frames = useFrames(error)
  // TODO: Select the nearest frame that's not ignore-listed when hiding ignore-listed
  // frames.
  const [isIgnoreListOpen, setIsIgnoreListOpen] = useState(false)

  // Find all frames that have code frames (can be displayed)
  const framesWithCodeFrame = useMemo(() => {
    return frames
      .map((frame, index) => ({ frame, index }))
      .filter(({ frame }) => Boolean(frame.originalCodeFrame))
  }, [frames])

  // Find the first non-ignored frame with code frame as the default selection
  // Don't fall back to ignored frames - match original behavior of showing nothing
  const defaultFrameIndex = useMemo(() => {
    const firstNonIgnored = framesWithCodeFrame.find(
      ({ frame }) => !frame.ignored
    )
    return firstNonIgnored?.index ?? null
  }, [framesWithCodeFrame])

  const [selectedFrameIndex, setSelectedFrameIndex] = useState<number | null>(
    defaultFrameIndex
  )

  // Reset selection when error changes
  useEffect(() => {
    setSelectedFrameIndex(defaultFrameIndex)
  }, [defaultFrameIndex])

  const selectedFrame = useMemo(() => {
    if (selectedFrameIndex === null) return null
    return frames[selectedFrameIndex] ?? null
  }, [frames, selectedFrameIndex])

  const handleFrameSelect = useCallback((index: number) => {
    setSelectedFrameIndex(index)
  }, [])

  // Only show codeframe if the selected frame is visible
  // (i.e., not ignored, or ignored but ignore list is open)
  const isSelectedFrameVisible =
    selectedFrame && (!selectedFrame.ignored || isIgnoreListOpen)

  return (
    <>
      {isSelectedFrameVisible &&
        selectedFrame?.originalStackFrame &&
        selectedFrame?.originalCodeFrame && (
          <CodeFrame
            stackFrame={selectedFrame.originalStackFrame}
            codeFrame={selectedFrame.originalCodeFrame}
          />
        )}

      {frames.length > 0 && (
        <ErrorOverlayCallStack
          dialogResizerRef={dialogResizerRef}
          frames={frames}
          selectedFrameIndex={selectedFrameIndex}
          onFrameSelect={handleFrameSelect}
          isIgnoreListOpen={isIgnoreListOpen}
          setIsIgnoreListOpen={setIsIgnoreListOpen}
        />
      )}

      {error.cause && (
        <ErrorCause cause={error.cause} dialogResizerRef={dialogResizerRef} />
      )}
    </>
  )
}

export const styles = `
  ${PSEUDO_HTML_DIFF_STYLES}
  ${errorCauseStyles}
`
