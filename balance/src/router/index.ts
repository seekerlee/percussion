import { createRouter, createWebHashHistory } from 'vue-router'

// Hash 模式：部署到任意静态服务都不用配 fallback，file:// 也能用
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: '/',
      name: '首页',
      component: () => import('../views/HomeView.vue'),
    },
    {
      path: '/anchor',
      name: 'Level 1 Anchor',
      meta: { nav: true },
      component: () => import('../views/AnchorView.vue'),
    },
    {
      path: '/xp-curve',
      name: 'XP Curve',
      meta: { nav: true },
      component: () => import('../views/XpCurveView.vue'),
    },
    {
      path: '/power-curve',
      name: 'Power Curve',
      meta: { nav: true },
      component: () => import('../views/PowerCurveView.vue'),
    },
  ],
})
