import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

// Vitest runs separately from the Vite build. jsdom provides a DOM for
// React Testing Library; setup.ts wires in @testing-library/jest-dom matchers.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    css: false,
  },
})
