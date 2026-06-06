<script setup lang="ts">
import { ref, computed } from 'vue'
import LineChart from '../components/LineChart.vue'
import type { ChartConfiguration } from 'chart.js/auto'
import { useLocalStorageRef } from '../composables/useLocalStorage'
import { D2_CUMULATIVE_XP, D2_XP_PER_LEVEL } from '../data/d2-xp-table'

// --- 参数 ---

const maxLevel = ref(99)
const skillsToMax = ref(2.3) // 0转满级 ≈ 练满 2.3 个技能

// 目标节奏（独立锚）
const targetMinStart = ref(20)
const targetMinEnd = ref(45)
const targetCurvature = ref(1.0)

// XP/分钟（乘数：当前是常数，未来可替换为函数）
const xpPerMinConst = ref(100)
const xpPerMinExp = ref(0) // 指数 j：0=常数, 1=线性, 2=平方

// 怪物密度（抽象量纲，单位以后绑定）：density(lv) = c × lv^j
const densityConst = ref(1)
const densityExp = ref(0)

// localStorage 持久化
useLocalStorageRef('balance:xp:maxLevel', maxLevel)
useLocalStorageRef('balance:xp:skillsToMax', skillsToMax)
useLocalStorageRef('balance:pacing:targetMinStart', targetMinStart)
useLocalStorageRef('balance:pacing:targetMinEnd', targetMinEnd)
useLocalStorageRef('balance:pacing:targetCurvature', targetCurvature)
useLocalStorageRef('balance:xpPerMin:const', xpPerMinConst)
useLocalStorageRef('balance:xpPerMin:exp', xpPerMinExp)
useLocalStorageRef('balance:density:const', densityConst)
useLocalStorageRef('balance:density:exp', densityExp)

// --- 计算 ---

const levels = computed(() =>
  Array.from({ length: maxLevel.value }, (_, i) => i + 1)
)

// 目标耗时曲线（独立锚）
const targetMinPerLevel = computed(() => {
  const start = targetMinStart.value
  const end = targetMinEnd.value
  const max = maxLevel.value
  const c = targetCurvature.value
  return levels.value.map(lv => {
    const t = (lv - 1) / (max - 1)
    const curved = Math.pow(t, c)
    return start * Math.pow(end / start, curved)
  })
})

// XP/分钟 = c × lv^j
const xpPerMinPerLevel = computed(() =>
  levels.value.map(lv => xpPerMinConst.value * Math.pow(lv, xpPerMinExp.value))
)

// 每级所需 XP（派生：耗时 × XP/分钟）
const xpPerLevel = computed(() =>
  levels.value.map((_, i) =>
    Math.round(targetMinPerLevel.value[i] * xpPerMinPerLevel.value[i])
  )
)

// 单技能满级累计 XP
const skillTotalXp = computed(() =>
  xpPerLevel.value.reduce((sum, xp) => sum + xp, 0)
)

// 角色满级总 XP = skillsToMax × 单技能满级 XP
const charTotalXp = computed(() => Math.round(skillsToMax.value * skillTotalXp.value))

// 累计 XP
const cumulativeXp = computed(() => {
  const arr: number[] = []
  let sum = 0
  for (const xp of xpPerLevel.value) {
    sum += xp
    arr.push(sum)
  }
  return arr
})

// D2 参考数据，对齐用户的 levels（用户 maxLevel 可能 != 99，多余补 null）
// 约定：index i 对应 Lv(i+1)，per-level = Lv(i+1)→Lv(i+2)，cumulative = 至 Lv(i+2) 总累计
const d2PerLevelAligned = computed<(number | null)[]>(() =>
  levels.value.map((_, i) => D2_XP_PER_LEVEL[i] ?? null)
)
const d2CumulativeAligned = computed<(number | null)[]>(() =>
  levels.value.map((_, i) => D2_CUMULATIVE_XP[i + 1] ?? null)
)

// 怪物密度（抽象量纲）：density(lv) = c × lv^j
const densityPerLevel = computed(() =>
  levels.value.map(lv => densityConst.value * Math.pow(lv, densityExp.value))
)

// 每只怪平均 XP（派生：xpPerMin / density）
const monsterXpPerLevel = computed(() =>
  levels.value.map((_, i) => {
    const d = densityPerLevel.value[i]
    return d > 0 ? xpPerMinPerLevel.value[i] / d : 0
  })
)

// 总耗时
const totalHours = computed(() =>
  targetMinPerLevel.value.reduce((sum, m) => sum + m, 0) / 60
)

// --- 图表 ---

// 共享 zoom 插件配置：滚轮缩放 + 拖拽平移，双击重置
const zoomOptions = {
  zoom: {
    wheel: { enabled: true },
    pinch: { enabled: true },
    drag: { enabled: false },
    mode: 'x' as const,
  },
  pan: {
    enabled: true,
    mode: 'x' as const,
  },
}

const pacingChart = computed<ChartConfiguration>(() => ({
  type: 'line',
  data: {
    labels: levels.value.map(String),
    datasets: [
      {
        label: '目标耗时 (分钟/级)',
        data: targetMinPerLevel.value,
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
      y: { title: { display: true, text: '分钟/级' } },
    },
    interaction: { mode: 'index', intersect: false },
    plugins: {
      zoom: zoomOptions,
      tooltip: {
        callbacks: {
          label: (ctx: any) => {
            const min = ctx.parsed.y
            if (min >= 60) return `${min.toFixed(0)} 分钟 (${(min / 60).toFixed(1)}h)`
            return `${min.toFixed(1)} 分钟`
          },
        },
      },
    },
  },
}))

const xpPerMinChart = computed<ChartConfiguration>(() => ({
  type: 'line',
  data: {
    labels: levels.value.map(String),
    datasets: [
      {
        label: 'XP/分钟',
        data: xpPerMinPerLevel.value,
        borderColor: '#7c3aed',
        tension: 0.3,
      },
    ],
  },
  options: {
    responsive: true,
    maintainAspectRatio: false,
    scales: {
      x: { title: { display: true, text: '等级' } },
      y: { title: { display: true, text: 'XP/分钟' }, beginAtZero: true, ticks: { callback: (v) => fmt(Number(v)) } },
    },
    interaction: { mode: 'index', intersect: false },
    plugins: {
      zoom: zoomOptions,
      tooltip: {
        callbacks: {
          label: (ctx: any) => `${fmt(ctx.parsed.y)} XP/分钟`,
        },
      },
    },
  },
}))

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
      {
        label: 'D2 每级 XP（参考）',
        data: d2PerLevelAligned.value,
        borderColor: '#2563eb',
        borderDash: [6, 4],
        borderWidth: 1.5,
        pointRadius: 0,
        tension: 0.3,
        yAxisID: 'y',
        hidden: false,
      },
      {
        label: 'D2 累计 XP（参考）',
        data: d2CumulativeAligned.value,
        borderColor: '#b0472e',
        borderDash: [6, 4],
        borderWidth: 1.5,
        pointRadius: 0,
        tension: 0.3,
        yAxisID: 'y1',
        hidden: false,
      },
    ],
  },
  options: {
    responsive: true,
    maintainAspectRatio: false,
    scales: {
      x: { title: { display: true, text: '等级' } },
      y: { type: 'linear', position: 'left', title: { display: true, text: '每级 XP' }, ticks: { callback: (v) => fmt(Number(v)) } },
      y1: { type: 'linear', position: 'right', title: { display: true, text: '累计 XP' }, grid: { drawOnChartArea: false }, ticks: { callback: (v) => fmt(Number(v)) } },
    },
    interaction: { mode: 'index', intersect: false },
    plugins: {
      zoom: zoomOptions,
      tooltip: {
        callbacks: {
          label: (ctx: any) => `${ctx.dataset.label}: ${fmt(ctx.parsed.y)}`,
        },
      },
    },
  },
}))

const monsterXpChart = computed<ChartConfiguration>(() => ({
  type: 'line',
  data: {
    labels: levels.value.map(String),
    datasets: [
      {
        label: '每只怪平均 XP',
        data: monsterXpPerLevel.value,
        borderColor: '#ea580c',
        tension: 0.3,
        yAxisID: 'y',
      },
      {
        label: '密度 density(lv)',
        data: densityPerLevel.value,
        borderColor: '#0891b2',
        borderDash: [4, 4],
        borderWidth: 1.5,
        pointRadius: 0,
        tension: 0.3,
        yAxisID: 'y1',
        hidden: true,
      },
    ],
  },
  options: {
    responsive: true,
    maintainAspectRatio: false,
    scales: {
      x: { title: { display: true, text: '等级' } },
      y: { type: 'linear', position: 'left', title: { display: true, text: '每只怪 XP' }, beginAtZero: true, ticks: { callback: (v) => fmt(Number(v)) } },
      y1: { type: 'linear', position: 'right', title: { display: true, text: '密度' }, grid: { drawOnChartArea: false }, ticks: { callback: (v) => fmt(Number(v)) } },
    },
    interaction: { mode: 'index', intersect: false },
    plugins: {
      zoom: zoomOptions,
      tooltip: {
        callbacks: {
          label: (ctx: any) => `${ctx.dataset.label}: ${fmt(ctx.parsed.y)}`,
        },
      },
    },
  },
}))

function fmt(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + 'B'
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toFixed(0)
}

// --- 导出 ---

const exportData = computed(() => ({
  schema: 'percussion.balance.xp-pacing',
  version: 1,
  generatedAt: new Date().toISOString(),
  config: {
    maxLevel: maxLevel.value,
    skillsToMax: skillsToMax.value,
    pacing: {
      formula: 'minutes(lv) = start × (end / start) ^ (((lv - 1) / (maxLv - 1)) ^ curvature)',
      substituted: `minutes(lv) = ${targetMinStart.value} × (${targetMinEnd.value} / ${targetMinStart.value}) ^ (((lv - 1) / ${maxLevel.value - 1}) ^ ${targetCurvature.value})`,
      startMin: targetMinStart.value,
      endMin: targetMinEnd.value,
      curvature: targetCurvature.value,
    },
    xpPerMin: {
      formula: 'xpPerMin(lv) = c × lv ^ j',
      substituted: `xpPerMin(lv) = ${xpPerMinConst.value} × lv ^ ${xpPerMinExp.value}`,
      c: xpPerMinConst.value,
      j: xpPerMinExp.value,
    },
    density: {
      formula: 'density(lv) = c × lv ^ j',
      substituted: `density(lv) = ${densityConst.value} × lv ^ ${densityExp.value}`,
      note: '抽象量纲，单位未绑定',
      c: densityConst.value,
      j: densityExp.value,
    },
  },
  derivedFormulas: {
    xpPerLevel: 'xp(lv) = minutes(lv) × xpPerMin(lv)',
    monsterXp: 'monsterXp(lv) = xpPerMin(lv) / density(lv)',
  },
  summary: {
    totalHours: Number(totalHours.value.toFixed(2)),
    totalDays: Number((totalHours.value / 24).toFixed(2)),
    skillTotalXp: skillTotalXp.value,
    charTotalXp: charTotalXp.value,
  },
}))

const exportJson = computed(() => JSON.stringify(exportData.value, null, 2))

const showExport = ref(false)

async function copyExport() {
  try {
    await navigator.clipboard.writeText(exportJson.value)
  } catch (e) {
    console.error('复制失败', e)
  }
}

function downloadExport() {
  const blob = new Blob([exportJson.value], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `xp-pacing-${new Date().toISOString().slice(0, 10)}.json`
  a.click()
  URL.revokeObjectURL(url)
}
</script>

<template>
  <div class="space-y-8">
    <div>
      <h2 class="text-xl font-bold mb-2">XP & Pacing — 经验与升级节奏</h2>
      <p class="text-muted">
        以时间为锚 → 定义 XP/分钟乘数 → 派生出经验需求曲线。
      </p>
    </div>

    <!-- ═══════════ 第一板块：升级节奏（锚） ═══════════ -->
    <section class="border-2 border-green-200 rounded-xl p-6 space-y-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="w-3 h-3 rounded-full bg-green-500"></div>
        <h3 class="text-lg font-bold">升级节奏目标</h3>
        <span class="text-sm text-muted">独立锚 · 玩家节奏体验</span>
      </div>

      <!-- 公式 -->
      <div class="bg-green-50 rounded-lg px-4 py-2 font-mono text-sm">
        <span class="font-bold">minutes(lv) = start × (end / start) ^ ((lv-1)/(maxLv-1))^curvature</span>
      </div>

      <!-- 参数 -->
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div>
          <label class="block text-sm text-muted mb-1">Lv1 升级: {{ targetMinStart }} 分钟</label>
          <input type="range" v-model.number="targetMinStart" min="5" max="60" step="1" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">Lv99 升级: {{ targetMinEnd >= 60 ? (targetMinEnd / 60).toFixed(1) + 'h' : targetMinEnd + '分钟' }}</label>
          <input type="range" v-model.number="targetMinEnd" min="10" max="1440" step="10" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">曲率: {{ targetCurvature.toFixed(1) }}</label>
          <input type="range" v-model.number="targetCurvature" min="0.5" max="4.0" step="0.1" class="w-full" />
          <div class="text-xs text-muted">1=标准指数, &gt;1 前平后陡, &lt;1 前期就涨</div>
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">最大等级: {{ maxLevel }}</label>
          <input type="range" v-model.number="maxLevel" min="10" max="99" class="w-full" />
        </div>
      </div>

      <!-- 关键数字 -->
      <div class="flex flex-wrap gap-6 text-sm">
        <div><span class="text-muted">满级总耗时:</span> <span class="font-mono font-bold text-accent">{{ totalHours.toFixed(1) }}h ({{ (totalHours / 24).toFixed(1) }}天)</span></div>
      </div>

      <!-- 图表 -->
      <div class="h-[280px]">
        <h4 class="text-sm font-semibold mb-2">每级耗时</h4>
        <LineChart :config="pacingChart" class="h-[250px]" />
      </div>
    </section>

    <!-- ═══════════ 第二板块：XP/分钟（乘数） ═══════════ -->
    <section class="border-2 border-purple-200 rounded-xl p-6 space-y-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="w-3 h-3 rounded-full bg-purple-500"></div>
        <h3 class="text-lg font-bold">XP/分钟（乘数）</h3>
        <span class="text-sm text-muted">独立锚 · 当前是常数，未来可替换为函数</span>
      </div>

      <!-- 公式 -->
      <div class="bg-purple-50 rounded-lg px-4 py-2 font-mono text-sm">
        <span class="font-bold">xpPerMin(lv) = c × lv ^ j</span>
        <span class="text-muted ml-2">// j=0 常数, j=1 线性, j=2 平方</span>
      </div>

      <!-- 参数 -->
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div>
          <label class="block text-sm text-muted mb-1">常数 c</label>
          <div class="flex gap-2 items-center">
            <input type="range" v-model.number="xpPerMinConst" min="0" max="10000" step="10" class="flex-1" />
            <input type="number" v-model.number="xpPerMinConst" min="0" max="10000" step="10" class="w-20 px-2 py-1 border rounded text-sm font-mono" />
          </div>
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">指数 j: {{ xpPerMinExp.toFixed(2) }}</label>
          <input type="range" v-model.number="xpPerMinExp" min="0" max="3.0" step="0.05" class="w-full" />
        </div>
      </div>

      <!-- 关键数字 -->
      <div class="flex flex-wrap gap-6 text-sm">
        <div><span class="text-muted">Lv1:</span> <span class="font-mono font-bold">{{ fmt(xpPerMinPerLevel[0] || 0) }} XP/min</span></div>
        <div><span class="text-muted">Lv50:</span> <span class="font-mono font-bold">{{ fmt(xpPerMinPerLevel[49] || 0) }} XP/min</span></div>
        <div><span class="text-muted">Lv99:</span> <span class="font-mono font-bold">{{ fmt(xpPerMinPerLevel[98] || 0) }} XP/min</span></div>
        <div><span class="text-muted">膨胀倍数:</span> <span class="font-mono font-bold">{{ (xpPerMinPerLevel[0] > 0 ? xpPerMinPerLevel[98] / xpPerMinPerLevel[0] : 0).toFixed(1) }}×</span></div>
      </div>

      <!-- 图表 -->
      <div class="h-[280px]">
        <h4 class="text-sm font-semibold mb-2">XP/分钟曲线</h4>
        <LineChart :config="xpPerMinChart" class="h-[250px]" />
      </div>
    </section>

    <!-- ═══════════ 第三板块：经验需求曲线（派生） ═══════════ -->
    <section class="border-2 border-blue-200 rounded-xl p-6 space-y-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="w-3 h-3 rounded-full bg-blue-500"></div>
        <h3 class="text-lg font-bold">经验需求曲线</h3>
        <span class="text-sm text-muted">派生 · 由前两节决定</span>
      </div>

      <!-- 公式 -->
      <div class="bg-blue-50 rounded-lg px-4 py-2 font-mono text-sm">
        <span class="font-bold">xp(lv) = minutes(lv) × xpPerMin(lv)</span>
      </div>

      <!-- 参数（仅 skillsToMax 影响角色总 XP 显示） -->
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div>
          <label class="block text-sm text-muted mb-1">满级≈练满技能数: {{ skillsToMax.toFixed(1) }}</label>
          <input type="range" v-model.number="skillsToMax" min="1.0" max="5.0" step="0.1" class="w-full" />
          <div class="text-xs text-muted">用于推算角色总 XP</div>
        </div>
      </div>

      <!-- 关键数字 -->
      <div class="flex flex-wrap gap-6 text-sm">
        <div><span class="text-muted">单技能满级 XP:</span> <span class="font-mono font-bold">{{ fmt(skillTotalXp) }}</span></div>
        <div><span class="text-muted">角色满级总 XP:</span> <span class="font-mono font-bold">{{ fmt(charTotalXp) }}</span></div>
        <div><span class="text-muted">Lv1 需:</span> <span class="font-mono">{{ fmt(xpPerLevel[0] || 0) }}</span></div>
        <div><span class="text-muted">Lv50 需:</span> <span class="font-mono">{{ fmt(xpPerLevel[49] || 0) }}</span></div>
        <div><span class="text-muted">Lv99 需:</span> <span class="font-mono">{{ fmt(xpPerLevel[98] || 0) }}</span></div>
      </div>

      <!-- 图表 -->
      <LineChart :config="xpCurveChart" class="h-[300px]" />
    </section>

    <!-- ═══════════ 第四板块：怪物经验（派生） ═══════════ -->
    <section class="border-2 border-orange-200 rounded-xl p-6 space-y-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="w-3 h-3 rounded-full bg-orange-500"></div>
        <h3 class="text-lg font-bold">怪物经验</h3>
        <span class="text-sm text-muted">派生 · 由 XP/分钟 ÷ 密度推出</span>
      </div>

      <!-- 公式 -->
      <div class="bg-orange-50 rounded-lg px-4 py-2 font-mono text-sm space-y-1">
        <div><span class="font-bold">density(lv) = c × lv ^ j</span> <span class="text-muted ml-2">// 抽象量纲，单位以后再绑（每分钟 / 单位面积 等）</span></div>
        <div><span class="font-bold">monsterXp(lv) = xpPerMin(lv) / density(lv)</span></div>
      </div>

      <!-- 参数 -->
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div>
          <label class="block text-sm text-muted mb-1">密度 常数 c</label>
          <div class="flex gap-2 items-center">
            <input type="range" v-model.number="densityConst" min="0.1" max="100" step="0.1" class="flex-1" />
            <input type="number" v-model.number="densityConst" min="0.1" max="10000" step="0.1" class="w-20 px-2 py-1 border rounded text-sm font-mono" />
          </div>
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">密度 指数 j: {{ densityExp.toFixed(2) }}</label>
          <input type="range" v-model.number="densityExp" min="0" max="3.0" step="0.05" class="w-full" />
        </div>
      </div>

      <!-- 关键数字 -->
      <div class="flex flex-wrap gap-6 text-sm">
        <div><span class="text-muted">Lv1 怪 XP:</span> <span class="font-mono font-bold">{{ fmt(monsterXpPerLevel[0] || 0) }}</span></div>
        <div><span class="text-muted">Lv50 怪 XP:</span> <span class="font-mono font-bold">{{ fmt(monsterXpPerLevel[49] || 0) }}</span></div>
        <div><span class="text-muted">Lv99 怪 XP:</span> <span class="font-mono font-bold">{{ fmt(monsterXpPerLevel[98] || 0) }}</span></div>
        <div><span class="text-muted">膨胀倍数:</span> <span class="font-mono font-bold">{{ (monsterXpPerLevel[0] > 0 ? monsterXpPerLevel[98] / monsterXpPerLevel[0] : 0).toFixed(1) }}×</span></div>
      </div>

      <!-- 图表 -->
      <div class="h-[280px]">
        <h4 class="text-sm font-semibold mb-2">每等级怪物平均 XP</h4>
        <LineChart :config="monsterXpChart" class="h-[250px]" />
      </div>
    </section>

    <!-- ═══════════ 导出 ═══════════ -->
    <section class="border-2 border-gray-300 rounded-xl p-6 space-y-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="w-3 h-3 rounded-full bg-gray-500"></div>
        <h3 class="text-lg font-bold">导出</h3>
        <span class="text-sm text-muted">所有参数 / 公式 / 每级数据</span>
        <div class="flex-1"></div>
        <button
          @click="showExport = !showExport"
          class="px-3 py-1 text-sm border rounded hover:bg-gray-50"
        >{{ showExport ? '隐藏预览' : '预览' }}</button>
        <button
          @click="copyExport"
          class="px-3 py-1 text-sm border rounded hover:bg-gray-50"
        >复制 JSON</button>
        <button
          @click="downloadExport"
          class="px-3 py-1 text-sm bg-ink text-white rounded hover:opacity-80"
        >下载 .json</button>
      </div>
      <pre
        v-if="showExport"
        class="bg-gray-50 border rounded p-3 text-xs font-mono overflow-auto max-h-[400px]"
      >{{ exportJson }}</pre>
    </section>

    <!-- 底部摘要条 -->
    <div class="bg-ink text-white rounded-lg px-6 py-3 flex items-center justify-between text-sm">
      <span>📌 摘要</span>
      <div class="flex gap-6 font-mono">
        <span>满级={{ totalHours.toFixed(1) }}h ({{ (totalHours / 24).toFixed(1) }}天)</span>
        <span>角色总XP={{ fmt(charTotalXp) }}</span>
        <span>Lv1={{ fmt(xpPerLevel[0] || 0) }}XP</span>
        <span>Lv99={{ fmt(xpPerLevel[98] || 0) }}XP</span>
      </div>
    </div>
  </div>
</template>
