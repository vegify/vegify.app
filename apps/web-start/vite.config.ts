import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'

import { tanstackStart } from '@tanstack/react-start/plugin/vite'

import viteReact from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const config = defineConfig({
  resolve: { tsconfigPaths: true },
  // native sqlite client can't live inside the SSR bundle
  ssr: { external: ['@libsql/client'] },
  plugins: [devtools(), tailwindcss(), tanstackStart(), viteReact()],
})

export default config
