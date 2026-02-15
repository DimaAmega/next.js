'use client'

import React, { startTransition, type JSX } from 'react'
import { HandleISRError } from './handle-isr-error'
import { handleHardNavError } from './nav-failure-handler'
import { useUntrackedPathname } from './navigation-untracked'
import { unstable_rethrow } from './unstable-rethrow'
import { isBot } from '../../shared/lib/router/utils/is-bot'
import {
  AppRouterContext,
  type AppRouterInstance,
} from '../../shared/lib/app-router-context.shared-runtime'
import { RouterContext } from '../../shared/lib/router-context.shared-runtime'
import type { ErrorInfo } from './error-boundary'

const isBotUserAgent =
  typeof window !== 'undefined' && isBot(window.navigator.userAgent)

export type FallbackComponent<P> = (
  props: P & { children?: React.ReactNode },
  errorInfo: ErrorInfo
) => React.ReactNode

type CatchErrorWrapperProps<P> = P & { children?: React.ReactNode }
type NextErrorBoundaryProps<P> = CatchErrorWrapperProps<P> & {
  pathname: string | null
  isPagesRouter: boolean
}

type NextErrorBoundaryState = {
  error: Error | null
  previousPathname: string | null
  componentStack: React.ErrorInfo['componentStack']
  ownerStack: ReturnType<typeof React.captureOwnerStack>
}

type FallbackWrapperProps<P> = {
  props: P
  errorInfo: ErrorInfo
}

function createFallbackWrapper<P extends Record<string, any>>(
  fallback: FallbackComponent<P>
): React.ComponentType<FallbackWrapperProps<P>> {
  const fallbackName = fallback.name || 'Unknown'
  const fallbackWrapper = {
    [fallbackName]: ({ props, errorInfo }: FallbackWrapperProps<P>) =>
      fallback(props, errorInfo),
  }[fallbackName] as React.ComponentType<FallbackWrapperProps<P>>

  if (process.env.NODE_ENV !== 'production') {
    fallbackWrapper.displayName = fallbackName
  }

  return fallbackWrapper
}

export function catchError<P extends Record<string, any>>(
  fallback: FallbackComponent<P>
): React.ComponentType<P & { children?: React.ReactNode }> {
  const FallbackWrapper = createFallbackWrapper(fallback)

  class NextErrorBoundary extends React.Component<
    NextErrorBoundaryProps<P>,
    NextErrorBoundaryState
  > {
    static contextType = AppRouterContext
    declare context: AppRouterInstance | null

    constructor(props: NextErrorBoundaryProps<P>) {
      super(props)
      this.state = {
        error: null,
        previousPathname: this.props.pathname,
        componentStack: undefined,
        ownerStack: null,
      }
    }

    static getDerivedStateFromError(error: Error): {
      error: Error
      ownerStack: ReturnType<typeof React.captureOwnerStack>
    } {
      unstable_rethrow(error)

      let ownerStack: string | null = null
      if ('captureOwnerStack' in React) {
        ownerStack = React.captureOwnerStack()
      }

      return { error, ownerStack }
    }

    componentDidCatch(_error: Error, errorInfo: React.ErrorInfo): void {
      this.setState({
        componentStack: errorInfo.componentStack,
      })
    }

    static getDerivedStateFromProps(
      props: NextErrorBoundaryProps<P>,
      state: NextErrorBoundaryState
    ): NextErrorBoundaryState | null {
      const { error } = state

      // If we encounter an error while a navigation is pending, don't render
      // the fallback and let the hard navigation attempt to recover instead.
      if (process.env.__NEXT_APP_NAV_FAIL_HANDLING) {
        if (error && handleHardNavError(error)) {
          return {
            error: null,
            previousPathname: props.pathname,
            componentStack: undefined,
            ownerStack: null,
          }
        }
      }

      // Reset error state when navigation changes pathname so boundaries don't
      // stay latched while navigating to a different route.
      if (props.pathname !== state.previousPathname && error) {
        return {
          error: null,
          previousPathname: props.pathname,
          componentStack: undefined,
          ownerStack: null,
        }
      }

      return {
        error: state.error,
        previousPathname: props.pathname,
        componentStack: state.componentStack,
        ownerStack: state.ownerStack,
      }
    }

    clearError = () => {
      this.setState({
        error: null,
        componentStack: undefined,
        ownerStack: null,
      })
    }

    reset = () => {
      this.clearError()
    }

    retry = () => {
      const router = this.context
      if (this.props.isPagesRouter || router === null) {
        throw new Error(
          '`retry()` can only be used in the App Router. Use `reset()` in the Pages Router.'
        )
      }

      startTransition(() => {
        router.refresh()
        this.clearError()
      })
    }

    render(): React.ReactNode {
      if (this.state.error && !isBotUserAgent) {
        const {
          children: _children,
          pathname: _pathname,
          ...componentProps
        } = this.props
        const errorInfo: ErrorInfo = {
          error: this.state.error,
          reset: this.reset,
          retry: this.retry,
          componentStack: this.state.componentStack,
          ownerStack: this.state.ownerStack,
        }

        return (
          <>
            <HandleISRError error={this.state.error} />
            <FallbackWrapper
              props={componentProps as unknown as P}
              errorInfo={errorInfo}
            />
          </>
        )
      }

      return this.props.children
    }
  }

  if (process.env.NODE_ENV !== 'production') {
    ;(NextErrorBoundary as any).displayName = 'Next.ErrorBoundary'
  }

  function CatchErrorWrapper(props: CatchErrorWrapperProps<P>): JSX.Element {
    const pathname = useUntrackedPathname()
    const pagesRouter = React.useContext(RouterContext)
    return (
      <NextErrorBoundary
        {...props}
        pathname={pathname}
        isPagesRouter={pagesRouter !== null}
      />
    )
  }

  if (process.env.NODE_ENV !== 'production') {
    const name = fallback.name || 'Unknown'
    CatchErrorWrapper.displayName = `catchError(${name})`
  }

  return CatchErrorWrapper
}
