import { defineConfig, presetUno, presetTypography } from 'unocss'

export default defineConfig({
  presets: [
    presetUno(),
    presetTypography(),
  ],
  theme: {
    colors: {
      ink: '#1d2430',
      muted: '#6b7280',
      accent: '#b0472e',
      panel: '#fffaf0',
      bg: '#f6f3ea',
    },
  },
})
