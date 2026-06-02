<script setup lang="ts">
import { RouterLink, RouterView } from 'vue-router'
</script>

<template>
  <div class="min-h-screen bg-bg text-ink font-serif">
    <nav class="border-b border-gray-200 px-6 py-3 flex items-center gap-6">
      <span class="font-bold text-lg text-accent">Percussion 数值策划</span>
      <RouterLink
        v-for="route in routes"
        :key="route.path"
        :to="route.path"
        class="text-muted hover:text-ink transition-colors"
        active-class="!text-accent font-semibold"
      >
        {{ route.name }}
      </RouterLink>
    </nav>
    <main class="p-6 max-w-7xl mx-auto">
      <RouterView />
    </main>
  </div>
</template>

<script lang="ts">
// 给 nav 列出所有工具页面（排除首页）
import { router } from './router'
const routes = router.getRoutes().filter(r => r.meta?.nav)
export default { data: () => ({ routes }) }
</script>
