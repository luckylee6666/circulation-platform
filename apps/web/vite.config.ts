import { fileURLToPath, URL } from 'node:url';

import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

/**
 * 一个工程两个入口：
 *   index.html   —— 局域网用户使用的网页端（产物会被内嵌进服务端二进制）
 *   console.html —— 桌面端控制台（作为 Tauri 的 frontendDist）
 */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        index: fileURLToPath(new URL('./index.html', import.meta.url)),
        console: fileURLToPath(new URL('./console.html', import.meta.url)),
      },
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    // 开发时把接口请求转发到本地运行的服务端
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
    },
  },
});
