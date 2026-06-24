import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'

import { tanstackStart } from '@tanstack/react-start/plugin/vite'

import viteReact from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const config = defineConfig({
  resolve: { tsconfigPaths: true },
  // Bundle every dep INTO the SSR build (incl. react + the @vegify/* workspace packages) so the
  // deployed server.js is self-contained; only the native @libsql/client stays external (it can't
  // be bundled and is the one thing the Lambda/Fargate bundle installs from node_modules).
  ssr: { external: ['@libsql/client'], noExternal: true },
  // React Compiler runs as a Babel plugin inside @vitejs/plugin-react (target React 19 → no runtime dep).
  plugins: [
    devtools(),
    tailwindcss(),
    tanstackStart(),
    viteReact({ babel: { plugins: [['babel-plugin-react-compiler', { target: '19' }]] } }),
  ],
})

export default config
