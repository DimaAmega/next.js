'use client'

import { catchError, type ErrorInfo } from 'next/error'
import Link from 'next/link'

function ErrorFallback(_props: {}, { error, reset }: ErrorInfo) {
  return (
    <>
      <p id="navigation-reset-error">{error.message}</p>
      <Link id="navigation-reset-safe-link" href="/navigation-reset/safe">
        Navigate to safe page
      </Link>
      <button id="navigation-reset-reset" onClick={() => reset()}>
        Reset
      </button>
    </>
  )
}

export default catchError(ErrorFallback)
