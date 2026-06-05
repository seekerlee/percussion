<script setup lang="ts">
import { ref, computed } from 'vue'
import LineChart from '../components/LineChart.vue'
import type { ChartConfiguration } from 'chart.js/auto'
import { useLocalStorageRef } from '../composables/useLocalStorage'

// --- 参数 ---
// 这条曲线既是技能 XP 曲线，也决定角色升级节奏
// 因为：角色 XP = ∑所有技能 XP，角色等级由累计 XP 推出
const maxLevel = ref(99)
const base = ref(100)
const k = ref(2.2)
const skillsToMax = ref(2.3) // 0转满级 ≈ 练满 2.3 个技能

// XP/分钟
const xpPerMinute = ref(50)

// localStorage 持久化
useLocalStorageRef('balance:xp:maxLevel', maxLevel)
useLocalStorageRef('balance:xp:base', base)
useLocalStorageRef('balance:xp:k', k)
useLocalStorageRef('balance:xp:xpPerMinute', xpPerMinute)

// --- 计算 ---

// 技能从 lv 升到 lv+1 所需 XP
function calcXpForLevel(lv: number): number {
  return base.value * Math.pow(lv, k.value)
}

// 每个技能等级所需 XP
const xpPerLevel = computed(() =>
  Array.from({ length: maxLevel.value }, (_, i) => Math.round(calcXpForLevel(i + 1)))
)

// 单技能满级累计 XP
const skillTotalXp = computed(() => {
  let sum = 0
  for (const xp of xpPerLevel.value) sum += xp
  return sum
})

// 角色满级总 XP = 2.3 × 单技能满级 XP
const charTotalXp = computed(() => Math.round(skillsToMax.value * skillTotalXp.value))

// 角色每级所需 XP（均匀分摊到 maxLevel 级，用同一多项式形状）
// 这里展示的是：如果角色也是99级，每级需要多少 XP
const charXpPerLevel = computed(() => {
  const total = charTotalXp.value
  // 按同样的多项式形状分配
  const rawWeights = Array.from({ length: maxLevel.value }, (_, i) => Math.pow(i + 1, k.value))
  const weightSum = rawWeights.reduce((a, b) => a + b, 0)
  return rawWeights.map(w => Math.round(total * w / weightSum))
})

const minutesPerLevel = computed(() =>
  xpPerLevel.value.map(xp => xp / xpPerMinute.value)
)

// 练满一个技能需要多少分钟
const minutesToMaxOneSkill = computed(() => skillTotalXp.value / xpPerMinute.value)

// 角色满级总时间（小时）
const hoursToMaxChar = computed(() => charTotalXp.value / xpPerMinute.value / 60)

// 累计 XP（用于图表）
const cumulativeXp = computed(() => {
  const arr: number[] = []
  let sum = 0
  for (const xp of xpPerLevel.value) {
    sum += xp
    arr.push(sum)
  }
  return arr
})

// --- 图表 ---
const levels = computed(() =>
  Array.from({ length: maxLevel.value }, (_, i) => i + 1)
)

const xpCurveChart = computed<ChartConfiguration>(() => ({
  type: 'line',
  data: {
    labels: levels.value.map(String),
    datasets: [
      {
        label: '每级所需 XP',
        data: xpPerLevel.value,
        borderColor: '#2563eb',
        tension: 0.3,
        yAxisID: 'y',
      },
      {
        label: '累计 XP',
        data: cumulativeXp.value,
        borderColor: '#b0472e',
        tension: 0.3,
        yAxisID: 'y1',
      },
    ],
  },
  options: {
    responsive: true,
    maintainAspectRatio: false,
    scales: {
      x: { title: { display: true, text: '等级' } },
      y: { type: 'linear', position: 'left', title: { display: true, text: '每级 XP' } },
      y1: { type: 'linear', position: 'right', title: { display: true, text: '累计 XP' }, grid: { drawOnChartArea: false } },
    },
    interaction: { mode: 'index', intersect: false },
  },
}))

const timeChart = computed<ChartConfiguration>(() => ({
  type: 'line',
  data: {
    labels: levels.value.map(String),
    datasets: [
      {
        label: '每级所需时间 (分钟)',
        data: minutesPerLevel.value,
        borderColor: '#15803d',
        tension: 0.3,
      },
    ],
  },
  options: {
    responsive: true,
    maintainAspectRatio: false,
    scales: {
      x: { title: { display: true, text: '等级' } },
      y: { title: { display: true, text: '分钟' } },
    },
    interaction: { mode: 'index', intersect: false },
  },
}))

function fmt(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toFixed(0)
}
</script>

<template>
  <div>
    <h2 class="text-xl font-bold mb-2">XP Curve — 经验曲线</h2>
    <p class="text-muted mb-4">
      调整参数，验证升级节奏和"0转99级 ≈ 2.3个技能满级"约束。
    </p>

    <!-- 公式展示区 -->
    <div class="bg-panel rounded-lg px-6 py-4 border border-gray-200 mb-6">
      <div class="text-xl font-mono font-bold text-accent tracking-wide">
        XP(lv) = base × lv ^ k
      </div>
      <div class="text-sm text-muted mt-1">
        = 从 lv 升到 lv+1 所需的经验（单级增量，非累计）
      </div>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-[300px_1fr] gap-6">
      <!-- 参数面板 -->
      <div class="bg-panel rounded-lg p-4 space-y-4 border border-gray-200">
        <h3 class="text-sm font-semibold text-accent">📐 参数</h3>
        <div>
          <label class="block text-sm text-muted mb-1">最大等级: {{ maxLevel }}</label>
          <input type="range" v-model.number="maxLevel" min="10" max="99" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">base: {{ base }}</label>
          <input type="range" v-model.number="base" min="10" max="500" step="10" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">指数 k: {{ k.toFixed(2) }}</label>
          <input type="range" v-model.number="k" min="1.0" max="3.5" step="0.05" class="w-full" />
        </div>

        <hr class="border-gray-200" />

        <h3 class="text-sm font-semibold text-accent">⏱️ 节奏假设</h3>
        <div>
          <label class="block text-sm text-muted mb-1">XP/分钟 (刷怪效率): {{ xpPerMinute }}</label>
          <input type="range" v-model.number="xpPerMinute" min="10" max="20000" step="5" class="w-full" />
        </div>

        <hr class="border-gray-200" />

        <h3 class="text-sm font-semibold text-accent">🎯 约束验证</h3>
        <p class="text-xs text-muted mb-2">角色总 XP = {{ skillsToMax }} × 单技能满级 XP</p>
        <div class="text-sm space-y-2">
          <div>
            <span class="text-muted">单技能满级 XP:</span>
            <span class="font-mono ml-2">{{ fmt(skillTotalXp) }}</span>
          </div>
          <div>
            <span class="text-muted">角色满级总 XP:</span>
            <span class="font-mono ml-2">{{ fmt(charTotalXp) }}</span>
          </div>
          <div>
            <span class="text-muted">单技能满级耗时:</span>
            <span class="font-mono ml-2">{{ (minutesToMaxOneSkill / 60).toFixed(1) }}h</span>
          </div>
          <div>
            <span class="text-muted">角色满级总耗时:</span>
            <span class="font-mono ml-2 font-bold text-accent">{{ hoursToMaxChar.toFixed(1) }}h</span>
          </div>
        </div>
      </div>

      <!-- 图表区 -->
      <div class="space-y-6">
        <div class="bg-panel rounded-lg p-4 border border-gray-200">
          <h3 class="font-semibold mb-2">经验需求曲线</h3>
          <LineChart :config="xpCurveChart" class="h-[350px]" />
        </div>
        <div class="bg-panel rounded-lg p-4 border border-gray-200">
          <h3 class="font-semibold mb-2">每级耗时 (分钟)</h3>
          <LineChart :config="timeChart" class="h-[300px]" />
        </div>
      </div>
    </div>

    <!-- 底部摘要条 -->
    <div class="mt-6 bg-ink text-white rounded-lg px-6 py-3 flex items-center justify-between text-sm">
      <span>📌 经验摘要</span>
      <div class="flex gap-6 font-mono">
        <span>Lv1需{{ fmt(xpPerLevel[0] || 0) }}XP</span>
        <span>Lv50需{{ fmt(xpPerLevel[49] || 0) }}XP</span>
        <span>Lv99需{{ fmt(xpPerLevel[98] || 0) }}XP</span>
        <span>满级={{ hoursToMaxChar.toFixed(1) }}h</span>
      </div>
    </div>
  </div>
</template>
