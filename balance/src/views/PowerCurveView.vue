<script setup lang="ts">
import { ref, computed } from 'vue'
import LineChart from '../components/LineChart.vue'
import type { ChartConfiguration } from 'chart.js/auto'

// --- 公式定义 ---

// 示例：线性 vs 多项式 vs 对数增长的技能伤害随等级曲线
const maxLevel = ref(20)
const baseHit = ref(10)
const linearK = ref(5)
const polyExp = ref(1.5)
const logK = ref(20)

const levels = computed(() =>
  Array.from({ length: maxLevel.value }, (_, i) => i + 1)
)

const linearDmg = computed(() =>
  levels.value.map(lv => baseHit.value + linearK.value * lv)
)

const polyDmg = computed(() =>
  levels.value.map(lv => baseHit.value + Math.pow(lv, polyExp.value))
)

const logDmg = computed(() =>
  levels.value.map(lv => baseHit.value + logK.value * Math.log(lv + 1))
)

const chartConfig = computed<ChartConfiguration>(() => ({
  type: 'line',
  data: {
    labels: levels.value.map(String),
    datasets: [
      {
        label: '线性',
        data: linearDmg.value,
        borderColor: '#2563eb',
        tension: 0.3,
      },
      {
        label: '多项式',
        data: polyDmg.value,
        borderColor: '#b0472e',
        tension: 0.3,
      },
      {
        label: '对数',
        data: logDmg.value,
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
      y: { title: { display: true, text: '伤害' } },
    },
    interaction: { mode: 'index', intersect: false },
  },
}))
</script>

<template>
  <div>
    <h2 class="text-xl font-bold mb-4">Skill Power Curve</h2>
    <p class="text-muted mb-4">调整参数，实时观察不同增长公式的伤害曲线差异。</p>

    <div class="grid grid-cols-1 md:grid-cols-[300px_1fr] gap-6">
      <!-- 参数面板 -->
      <div class="bg-panel rounded-lg p-4 space-y-4 border border-gray-200">
        <div>
          <label class="block text-sm text-muted mb-1">最大等级: {{ maxLevel }}</label>
          <input type="range" v-model.number="maxLevel" min="5" max="50" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">基础伤害: {{ baseHit }}</label>
          <input type="range" v-model.number="baseHit" min="0" max="50" class="w-full" />
        </div>
        <hr class="border-gray-200" />
        <div>
          <label class="block text-sm text-muted mb-1">线性系数 k: {{ linearK }}</label>
          <input type="range" v-model.number="linearK" min="1" max="20" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">多项式指数: {{ polyExp.toFixed(1) }}</label>
          <input type="range" v-model.number="polyExp" min="1" max="3" step="0.1" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">对数系数: {{ logK }}</label>
          <input type="range" v-model.number="logK" min="5" max="50" class="w-full" />
        </div>
      </div>

      <!-- 图表 -->
      <LineChart :config="chartConfig" class="h-[400px]" />
    </div>
  </div>
</template>
