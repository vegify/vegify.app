import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'

import { tanstackStart } from '@tanstack/react-start/plugin/vite'

import viteReact from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const config = defineConfig({
  resolve: { tsconfigPaths: true },
  // native sqlite client can't live inside the SSR bundle
  ssr: { external: ['@libsql/client'] },
  // React Compiler runs as a Babel plugin inside @vitejs/plugin-react (target React 19 → no runtime dep).
  plugins: [
    devtools(),
    tailwindcss(),
    tanstackStart(),
    viteReact({ babel: { plugins: [['babel-plugin-react-compiler', { target: '19' }]] } }),
  ],
})

export default config
