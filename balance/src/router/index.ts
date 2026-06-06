import { createRouter, createWebHashHistory } from 'vue-router'

// Hash 模式：部署到任意静态服务都不用配 fallback，file:// 也能用
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: '/',
      name: 'XP & Pacing',
      meta: { nav: true },
      component: () => import('../views/PacingView.vue'),
    },
  ],
})
