<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, shallowRef } from 'vue'
import { Chart, type ChartConfiguration } from 'chart.js/auto'

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
    chart.value.data = cfg.data
    if (cfg.options) chart.value.options = cfg.options
    chart.value.update()
  },
  { deep: true }
)
</script>

<template>
  <div class="relative w-full h-full min-h-[300px]">
    <canvas ref="canvasRef"></canvas>
  </div>
</template>
