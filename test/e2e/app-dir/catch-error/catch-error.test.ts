import { nextTestSetup } from 'e2e-utils'

/**
 * Normalize a React component stack string into an array of component names.
 * Strips URLs, line numbers, and other dynamic parts.
 */
function normalizeComponentStack(stack: string): string[] {
  // Match "at ComponentName" patterns, optionally with [Server] tag
  return stack.match(/at \w+(?:\s+\[Server\])?/g) || []
}

describe('app-dir - catchError', () => {
  const { next, isNextDev } = nextTestSetup({
    files: __dirname,
  })

  describe('Client Component error', () => {
    it('should recover Client Component error after retry', async () => {
      const browser = await next.browser('/client-component')

      // Try triggering and retrying a few times in a row
      for (let i = 0; i < 5; i++) {
        await browser
          .elementByCss('#error-trigger-button')
          .click()
          .waitForElementByCss('#error-boundary-message')

        expect(
          await browser.elementByCss('#error-boundary-message').text()
        ).toBe('this is a test')

        await browser
          .elementByCss('#retry')
          .click()
          .waitForElementByCss('#error-trigger-button')

        expect(await browser.elementByCss('#error-trigger-button').text()).toBe(
          'Trigger Error!'
        )
      }
    })

    it('should recover Client Component error after reset', async () => {
      const browser = await next.browser('/client-component')

      // Try triggering and resetting a few times in a row
      for (let i = 0; i < 5; i++) {
        await browser
          .elementByCss('#error-trigger-button')
          .click()
          .waitForElementByCss('#error-boundary-message')

        expect(
          await browser.elementByCss('#error-boundary-message').text()
        ).toBe('this is a test')

        await browser
          .elementByCss('#reset')
          .click()
          .waitForElementByCss('#error-trigger-button')

        expect(await browser.elementByCss('#error-trigger-button').text()).toBe(
          'Trigger Error!'
        )
      }
    })
  })

  describe('Server Component error', () => {
    it('should recover Server Component error after retry', async () => {
      const browser = await next.browser('/server-component/recover')

      expect(await browser.elementByCss('#error-boundary-message').text()).toBe(
        isNextDev
          ? 'this is a test'
          : 'An error occurred in the Server Components render. The specific message is omitted in production builds to avoid leaking sensitive details. A digest property is included on this error instance which may provide additional details about the nature of the error.'
      )

      await browser
        .elementByCss('#retry')
        .click()
        .waitForElementByCss('#recover')

      expect(await browser.elementByCss('#recover').text()).toBe('Recovered')
    })
  })

  it('should reset boundary state when navigating to a sibling route', async () => {
    const browser = await next.browser('/navigation-reset/error')

    await browser.waitForElementByCss('#navigation-reset-error')

    expect(await browser.elementByCss('#navigation-reset-error').text()).toBe(
      isNextDev
        ? 'navigation reset test'
        : 'An error occurred in the Server Components render. The specific message is omitted in production builds to avoid leaking sensitive details. A digest property is included on this error instance which may provide additional details about the nature of the error.'
    )

    await browser.elementByCss('#navigation-reset-safe-link').click()
    await browser.waitForElementByCss('#navigation-reset-safe')

    expect(await browser.elementByCss('#navigation-reset-safe').text()).toBe(
      'Navigation reset safe page'
    )
  })

  describe('Pages Router error', () => {
    it('should recover after reset', async () => {
      const browser = await next.browser('/pages-router')

      await browser
        .elementByCss('#pages-trigger')
        .click()
        .waitForElementByCss('#pages-error-message')

      expect(await browser.elementByCss('#pages-error-message').text()).toBe(
        'this is a pages test'
      )

      await browser.eval(`document.getElementById('pages-reset')?.click()`)
      await browser.waitForElementByCss('#pages-trigger')

      expect(await browser.elementByCss('#pages-trigger').text()).toBe(
        'Trigger Error!'
      )
    })

    it('should throw when retry is called', async () => {
      const browser = await next.browser('/pages-router')

      await browser
        .elementByCss('#pages-trigger')
        .click()
        .waitForElementByCss('#pages-error-message')

      await browser.eval(`document.getElementById('pages-retry')?.click()`)
      await browser.waitForElementByCss('#pages-retry-error')

      expect(await browser.elementByCss('#pages-retry-error').text()).toBe(
        '`retry()` can only be used in the App Router. Use `reset()` in the Pages Router.'
      )
    })
  })

  it('should pass componentStack and ownerStack to error component', async () => {
    const browser = await next.browser('/client-component')

    await browser
      .elementByCss('#error-trigger-button')
      .click()
      .waitForElementByCss('#error-component-stack')

    if (isNextDev) {
      const componentStack = normalizeComponentStack(
        await browser.elementByCss('#error-component-stack').text()
      )

      if (process.env.__NEXT_CACHE_COMPONENTS === 'true') {
        expect(componentStack).toMatchInlineSnapshot(`
         [
           "at Page",
           "at ClientPageRoot",
           "at SegmentViewNode",
           "at InnerLayoutRouter",
           "at RedirectErrorBoundary",
           "at RedirectBoundary",
           "at HTTPAccessFallbackBoundary",
           "at LoadingBoundary",
           "at ErrorBoundary",
           "at InnerScrollAndFocusHandlerOld",
           "at ScrollAndFocusHandler",
           "at RenderFromTemplateContext",
           "at SegmentStateProvider",
           "at Activity",
           "at OuterLayoutRouter",
           "at NextErrorBoundary",
           "at CatchErrorWrapper",
           "at Layout",
           "at SegmentViewNode",
           "at InnerLayoutRouter",
           "at RedirectErrorBoundary",
           "at RedirectBoundary",
           "at HTTPAccessFallbackErrorBoundary",
           "at HTTPAccessFallbackBoundary",
           "at LoadingBoundary",
           "at ErrorBoundary",
           "at InnerScrollAndFocusHandlerOld",
           "at ScrollAndFocusHandler",
           "at RenderFromTemplateContext",
           "at SegmentStateProvider",
           "at Activity",
           "at OuterLayoutRouter",
           "at body",
           "at html",
           "at RootLayout",
           "at SegmentViewNode",
           "at __next_root_layout_boundary__",
           "at RedirectErrorBoundary",
           "at RedirectBoundary",
           "at HTTPAccessFallbackErrorBoundary",
           "at HTTPAccessFallbackBoundary",
           "at DevRootHTTPAccessFallbackBoundary",
           "at AppDevOverlayErrorBoundary",
           "at HotReload",
           "at Router",
           "at ErrorBoundaryHandler",
           "at ErrorBoundary",
           "at RootErrorBoundary",
           "at AppRouter",
           "at ServerRoot",
           "at Root",
         ]
        `)
      } else {
        if (process.env.IS_WEBPACK_TEST) {
          expect(componentStack).toMatchInlineSnapshot(`
          [
            "at Page",
            "at ClientPageRoot",
            "at SegmentViewNode",
            "at InnerLayoutRouter",
            "at RedirectErrorBoundary",
            "at RedirectBoundary",
            "at HTTPAccessFallbackBoundary",
            "at LoadingBoundary",
            "at ErrorBoundary",
            "at InnerScrollAndFocusHandlerOld",
            "at ScrollAndFocusHandler",
            "at RenderFromTemplateContext",
            "at SegmentStateProvider",
            "at OuterLayoutRouter",
            "at NextErrorBoundary",
            "at CatchErrorWrapper",
            "at Layout [Server]",
            "at SegmentViewNode",
            "at InnerLayoutRouter",
            "at RedirectErrorBoundary",
            "at RedirectBoundary",
            "at HTTPAccessFallbackErrorBoundary",
            "at HTTPAccessFallbackBoundary",
            "at LoadingBoundary",
            "at ErrorBoundary",
            "at InnerScrollAndFocusHandlerOld",
            "at ScrollAndFocusHandler",
            "at RenderFromTemplateContext",
            "at SegmentStateProvider",
            "at OuterLayoutRouter",
            "at body",
            "at html",
            "at RootLayout [Server]",
            "at SegmentViewNode",
            "at __next_root_layout_boundary__",
            "at RedirectErrorBoundary",
            "at RedirectBoundary",
            "at HTTPAccessFallbackErrorBoundary",
            "at HTTPAccessFallbackBoundary",
            "at DevRootHTTPAccessFallbackBoundary",
            "at AppDevOverlayErrorBoundary",
            "at HotReload",
            "at Router",
            "at ErrorBoundaryHandler",
            "at ErrorBoundary",
            "at RootErrorBoundary",
            "at AppRouter",
            "at ServerRoot",
            "at Root",
          ]
        `)
        } else {
          expect(componentStack).toMatchInlineSnapshot(`
        [
          "at Page",
          "at ClientPageRoot",
          "at SegmentViewNode",
          "at InnerLayoutRouter",
          "at RedirectErrorBoundary",
          "at RedirectBoundary",
          "at HTTPAccessFallbackBoundary",
          "at LoadingBoundary",
          "at ErrorBoundary",
          "at InnerScrollAndFocusHandlerOld",
          "at ScrollAndFocusHandler",
          "at RenderFromTemplateContext",
          "at SegmentStateProvider",
          "at OuterLayoutRouter",
          "at NextErrorBoundary",
          "at CatchErrorWrapper",
          "at Layout [Server]",
          "at SegmentViewNode",
          "at InnerLayoutRouter",
          "at RedirectErrorBoundary",
          "at RedirectBoundary",
          "at HTTPAccessFallbackErrorBoundary",
          "at HTTPAccessFallbackBoundary",
          "at LoadingBoundary",
          "at ErrorBoundary",
          "at InnerScrollAndFocusHandlerOld",
          "at ScrollAndFocusHandler",
          "at RenderFromTemplateContext",
          "at SegmentStateProvider",
          "at OuterLayoutRouter",
          "at body",
          "at html",
          "at RootLayout [Server]",
          "at SegmentViewNode",
          "at __next_root_layout_boundary__",
          "at RedirectErrorBoundary",
          "at RedirectBoundary",
          "at HTTPAccessFallbackErrorBoundary",
          "at HTTPAccessFallbackBoundary",
          "at DevRootHTTPAccessFallbackBoundary",
          "at AppDevOverlayErrorBoundary",
          "at HotReload",
          "at Router",
          "at ErrorBoundaryHandler",
          "at ErrorBoundary",
          "at RootErrorBoundary",
          "at AppRouter",
          "at ServerRoot",
          "at Root",
        ]
      `)
        }
      }

      expect(
        normalizeComponentStack(
          await browser.elementByCss('#error-owner-stack').text()
        )
      ).toMatchInlineSnapshot(`
       [
         "at CatchErrorWrapper",
         "at Layout",
       ]
      `)
    }
  })
})
