import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'

export default defineConfig({
  plugins: [vue(), UnoCSS()],
  // 生产 build 输出的 asset 路径用相对路径，方便部署到任意子目录
  base: './',
})
