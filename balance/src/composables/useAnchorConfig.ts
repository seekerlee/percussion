import { reactive, computed } from 'vue'
import { useLocalStorage } from './useLocalStorage'

/**
 * Level 1 锚点场景的全局共享状态。
 * 其他页面（Combat Formula、Monster Scaling 等）可以引用这里定义的基准值。
 */

export interface AnchorConfig {
  // 玩家 Level 1 基准
  playerHp: number
  playerDps: number // 每秒伤害输出

  // 怪物 Level 1 基准（普通怪）
  monsterHp: number
  monsterDps: number

  // 精英/BOSS 倍数
  eliteHpMul: number
  eliteAtkMul: number
  bossHpMul: number
  bossAtkMul: number
}

const state = reactive<AnchorConfig>({
  playerHp: 100,
  playerDps: 30,
  monsterHp: 80,
  monsterDps: 10,
  eliteHpMul: 4,
  eliteAtkMul: 1.8,
  bossHpMul: 15,
  bossAtkMul: 2.5,
})

useLocalStorage('balance:anchor', state)

export function useAnchorConfig() {
  // TTK = Time To Kill (秒)
  const ttkPlayerKillsNormal = computed(() => state.monsterHp / state.playerDps)
  const ttkNormalKillsPlayer = computed(() => state.playerHp / state.monsterDps)
  const ttkRatioNormal = computed(() => ttkNormalKillsPlayer.value / ttkPlayerKillsNormal.value)

  const ttkPlayerKillsElite = computed(() => (state.monsterHp * state.eliteHpMul) / state.playerDps)
  const ttkEliteKillsPlayer = computed(() => state.playerHp / (state.monsterDps * state.eliteAtkMul))
  const ttkRatioElite = computed(() => ttkEliteKillsPlayer.value / ttkPlayerKillsElite.value)

  const ttkPlayerKillsBoss = computed(() => (state.monsterHp * state.bossHpMul) / state.playerDps)
  const ttkBossKillsPlayer = computed(() => state.playerHp / (state.monsterDps * state.bossAtkMul))
  const ttkRatioBoss = computed(() => ttkBossKillsPlayer.value / ttkPlayerKillsBoss.value)

  return {
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
  }
}
