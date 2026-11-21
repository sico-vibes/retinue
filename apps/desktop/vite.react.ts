import react from '@vitejs/plugin-react'

// React with React Compiler (21 November 2026, the owner's decision after
// docs/research/performance-2026-11.md): the compiler memoises components and
// hooks automatically, so a keystroke or a streamed chunk stops re-rendering
// every row. A component that breaks the Rules of React is skipped, not broken.
// The unit tests run without it: compiling the 4,700-line App.tsx under a busy
// worker timed out Vitest's module fetch, and the tests check behaviour, which
// the compiler must not change. The production build and the preview
// benchmark run the compiled code.
export const reactWithCompiler = () => process.env.VITEST ? react() : react({ babel: { plugins: [['babel-plugin-react-compiler', { target: '19' }]] } })
