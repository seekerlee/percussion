<script setup lang="ts">
import { useAnchorConfig } from '../composables/useAnchorConfig'

const {
  state,
  ttkPlayerKillsNormal,
  ttkNormalKillsPlayer,
  ttkRatioNormal,
  ttkPlayerKillsElite,
  ttkEliteKillsPlayer,
  ttkRatioElite,
  ttkPlayerKillsBoss,
  ttkBossKillsPlayer,
  ttkRatioBoss,
} = useAnchorConfig()

// 预设方案
function applyPreset(preset: 'conservative' | 'balanced' | 'aggressive') {
  const presets = {
    conservative: { playerHp: 120, playerDps: 20, monsterHp: 60, monsterDps: 8 },
    balanced: { playerHp: 100, playerDps: 30, monsterHp: 80, monsterDps: 10 },
    aggressive: { playerHp: 80, playerDps: 45, monsterHp: 100, monsterDps: 15 },
  }
  Object.assign(state, presets[preset])
}

function fmt(n: number): string {
  return n.toFixed(2)
}
</script>

<template>
  <div>
    <h2 class="text-xl font-bold mb-2">Level 1 Anchor — 锚点场景</h2>
    <p class="text-muted mb-4">
      定义"Level 1 玩家 vs 第一个怪物"的基准数值。所有其他数值从这里辐射。
    </p>

    <div class="grid grid-cols-1 lg:grid-cols-[320px_1fr] gap-6">
      <!-- 参数面板 -->
      <div class="bg-panel rounded-lg p-4 space-y-4 border border-gray-200">
        <!-- 预设 -->
        <div>
          <span class="text-sm text-muted block mb-2">快速预设</span>
          <div class="flex gap-2">
            <button @click="applyPreset('conservative')" class="btn">保守</button>
            <button @click="applyPreset('balanced')" class="btn btn-active">中庸</button>
            <button @click="applyPreset('aggressive')" class="btn">激进</button>
          </div>
        </div>

        <hr class="border-gray-200" />

        <!-- 玩家参数 -->
        <h3 class="text-sm font-semibold text-accent">🧑 玩家 (Level 1)</h3>
        <div>
          <label class="block text-sm text-muted mb-1">HP: {{ state.playerHp }}</label>
          <input type="range" v-model.number="state.playerHp" min="20" max="500" step="10" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">DPS: {{ state.playerDps }}</label>
          <input type="range" v-model.number="state.playerDps" min="5" max="100" step="1" class="w-full" />
        </div>

        <hr class="border-gray-200" />

        <!-- 怪物参数 -->
        <h3 class="text-sm font-semibold text-accent">👾 普通怪 (Level 1)</h3>
        <div>
          <label class="block text-sm text-muted mb-1">HP: {{ state.monsterHp }}</label>
          <input type="range" v-model.number="state.monsterHp" min="10" max="300" step="10" class="w-full" />
        </div>
        <div>
          <label class="block text-sm text-muted mb-1">DPS: {{ state.monsterDps }}</label>
          <input type="range" v-model.number="state.monsterDps" min="1" max="50" step="1" class="w-full" />
        </div>

        <hr class="border-gray-200" />

        <!-- Tier 倍数 -->
        <h3 class="text-sm font-semibold text-accent">⚔️ Tier 倍数</h3>
        <div class="grid grid-cols-2 gap-2 text-sm">
          <div>
            <label class="text-muted">精英 HP ×</label>
            <input type="number" v-model.number="state.eliteHpMul" min="1" max="20" step="0.5"
              class="w-full border rounded px-2 py-1" />
          </div>
          <div>
            <label class="text-muted">精英 ATK ×</label>
            <input type="number" v-model.number="state.eliteAtkMul" min="1" max="10" step="0.1"
              class="w-full border rounded px-2 py-1" />
          </div>
          <div>
            <label class="text-muted">BOSS HP ×</label>
            <input type="number" v-model.number="state.bossHpMul" min="1" max="50" step="1"
              class="w-full border rounded px-2 py-1" />
          </div>
          <div>
            <label class="text-muted">BOSS ATK ×</label>
            <input type="number" v-model.number="state.bossAtkMul" min="1" max="10" step="0.1"
              class="w-full border rounded px-2 py-1" />
          </div>
        </div>
      </div>

      <!-- 结果面板 -->
      <div class="space-y-4">
        <!-- TTK 表格 -->
        <div class="bg-panel rounded-lg p-4 border border-gray-200">
          <h3 class="font-semibold mb-3">⏱️ Time To Kill (TTK)</h3>
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-gray-200">
                <th class="text-left py-2">对手</th>
                <th class="text-right py-2">玩家击杀</th>
                <th class="text-right py-2">被击杀</th>
                <th class="text-right py-2">安全比</th>
                <th class="text-right py-2">判定</th>
              </tr>
            </thead>
            <tbody>
              <tr class="border-b border-gray-100">
                <td class="py-2">👾 普通怪</td>
                <td class="text-right font-mono">{{ fmt(ttkPlayerKillsNormal) }}s</td>
                <td class="text-right font-mono">{{ fmt(ttkNormalKillsPlayer) }}s</td>
                <td class="text-right font-mono">{{ fmt(ttkRatioNormal) }}×</td>
                <td class="text-right">
                  <span v-if="ttkRatioNormal >= 3" class="text-green-600">✓ 安全</span>
                  <span v-else-if="ttkRatioNormal >= 1.5" class="text-yellow-600">⚠ 紧张</span>
                  <span v-else class="text-red-600">✗ 危险</span>
                </td>
              </tr>
              <tr class="border-b border-gray-100">
                <td class="py-2">⚔️ 精英</td>
                <td class="text-right font-mono">{{ fmt(ttkPlayerKillsElite) }}s</td>
                <td class="text-right font-mono">{{ fmt(ttkEliteKillsPlayer) }}s</td>
                <td class="text-right font-mono">{{ fmt(ttkRatioElite) }}×</td>
                <td class="text-right">
                  <span v-if="ttkRatioElite >= 1.5" class="text-green-600">✓ 可控</span>
                  <span v-else-if="ttkRatioElite >= 0.8" class="text-yellow-600">⚠ 硬仗</span>
                  <span v-else class="text-red-600">✗ 必死</span>
                </td>
              </tr>
              <tr>
                <td class="py-2">🐉 BOSS</td>
                <td class="text-right font-mono">{{ fmt(ttkPlayerKillsBoss) }}s</td>
                <td class="text-right font-mono">{{ fmt(ttkBossKillsPlayer) }}s</td>
                <td class="text-right font-mono">{{ fmt(ttkRatioBoss) }}×</td>
                <td class="text-right">
                  <span v-if="ttkRatioBoss >= 0.8" class="text-green-600">✓ 可磨</span>
                  <span v-else-if="ttkRatioBoss >= 0.3" class="text-yellow-600">⚠ 需操作</span>
                  <span v-else class="text-red-600">✗ 不合理</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- 直觉说明 -->
        <div class="bg-panel rounded-lg p-4 border border-gray-200">
          <h3 class="font-semibold mb-3">📖 读数说明</h3>
          <ul class="text-sm text-muted space-y-1 list-disc list-inside">
            <li><strong>玩家击杀</strong> = 怪物HP ÷ 玩家DPS（纯输出，不考虑命中/暴击）</li>
            <li><strong>被击杀</strong> = 玩家HP ÷ 怪物DPS（纯承伤，不考虑回复/闪避）</li>
            <li><strong>安全比</strong> = 被击杀时间 ÷ 击杀时间（越高越安全，&lt;1 意味着先死）</li>
            <li>框架目标：普通怪 TTK ≈ 3-6次行动，安全比 &gt; 3×</li>
            <li>这些是<em>裸数值</em>，实际战斗还有命中/暴击/回复/多怪等因素</li>
          </ul>
        </div>

        <!-- 行动次数参考 -->
        <div class="bg-panel rounded-lg p-4 border border-gray-200">
          <h3 class="font-semibold mb-3">🎯 行动次数参考（假设 1 action/s）</h3>
          <div class="grid grid-cols-3 gap-4 text-center">
            <div>
              <div class="text-2xl font-bold text-accent">{{ fmt(ttkPlayerKillsNormal) }}</div>
              <div class="text-xs text-muted">次行动杀普通怪</div>
              <div class="text-xs" :class="ttkPlayerKillsNormal >= 3 && ttkPlayerKillsNormal <= 6 ? 'text-green-600' : 'text-yellow-600'">
                目标: 3~6
              </div>
            </div>
            <div>
              <div class="text-2xl font-bold text-accent">{{ fmt(ttkPlayerKillsElite) }}</div>
              <div class="text-xs text-muted">次行动杀精英</div>
              <div class="text-xs" :class="ttkPlayerKillsElite >= 8 && ttkPlayerKillsElite <= 15 ? 'text-green-600' : 'text-yellow-600'">
                目标: 8~15
              </div>
            </div>
            <div>
              <div class="text-2xl font-bold text-accent">{{ fmt(ttkPlayerKillsBoss) }}</div>
              <div class="text-xs text-muted">次行动杀BOSS</div>
              <div class="text-xs" :class="ttkPlayerKillsBoss >= 20 && ttkPlayerKillsBoss <= 50 ? 'text-green-600' : 'text-yellow-600'">
                目标: 20~50
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 底部摘要条 -->
    <div class="mt-6 bg-ink text-white rounded-lg px-6 py-3 flex items-center justify-between text-sm">
      <span>📌 锚点摘要</span>
      <div class="flex gap-6 font-mono">
        <span>普通怪 TTK={{ fmt(ttkPlayerKillsNormal) }}s</span>
        <span>安全比={{ fmt(ttkRatioNormal) }}×</span>
        <span>精英 TTK={{ fmt(ttkPlayerKillsElite) }}s</span>
        <span>BOSS TTK={{ fmt(ttkPlayerKillsBoss) }}s</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.btn {
  padding: 0.25rem 0.75rem;
  border-radius: 0.375rem;
  border: 1px solid #d1d5db;
  font-size: 0.875rem;
  cursor: pointer;
  transition: all 0.15s;
}
.btn:hover {
  border-color: #b0472e;
  color: #b0472e;
}
.btn-active {
  background: #b0472e;
  color: white;
  border-color: #b0472e;
}
</style>
