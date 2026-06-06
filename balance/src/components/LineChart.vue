<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, shallowRef } from 'vue'
import { Chart, type ChartConfiguration } from 'chart.js/auto'
import zoomPlugin from 'chartjs-plugin-zoom'

Chart.register(zoomPlugin)

const props = defineProps<{ config: ChartConfiguration }>()

const canvasRef = ref<HTMLCanvasElement>()
const chart = shallowRef<Chart>()

onMounted(() => {
  if (!canvasRef.value) return
  chart.value = new Chart(canvasRef.value, props.config)
})

onUnmounted(() => {
  chart.value?.destroy()
})

watch(
  () => props.config,
  (cfg) => {
    if (!chart.value) return
    // 保存用户通过 legend 切换的可见性，避免 config 更新时被重置
    const visibility = chart.value.data.datasets.map((_, i) =>
      chart.value!.isDatasetVisible(i)
    )
    chart.value.data = cfg.data
    if (cfg.options) chart.value.options = cfg.options
    chart.value.update()
    visibility.forEach((visible, i) => {
      if (i < chart.value!.data.datasets.length) {
        chart.value!.setDatasetVisibility(i, visible)
      }
    })
    chart.value.update()
  },
  { deep: true }
)
</script>

<template>
  <div class="relative w-full h-full min-h-[300px] overflow-hidden" style="max-height: inherit;">
    <canvas ref="canvasRef" @dblclick="chart?.resetZoom()"></canvas>
  </div>
</template>
