import { Suspense } from 'react'
import { connection } from 'next/server'

export default function Page() {
  return (
    <Suspense>
      <PageImpl />
    </Suspense>
  )
}

async function PageImpl() {
  // Throw only during the runtime
  await connection()

  throw new Error('navigation reset test')

  return null
}
