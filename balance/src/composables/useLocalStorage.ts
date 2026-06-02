import { watch, type Ref } from 'vue'

/**
 * 将一个 reactive 对象或 ref 自动同步到 localStorage。
 * 页面加载时从 localStorage 恢复，之后每次变化自动写入。
 */
export function useLocalStorage<T extends object>(key: string, state: T): void {
  // 恢复
  const saved = localStorage.getItem(key)
  if (saved) {
    try {
      const parsed = JSON.parse(saved)
      Object.assign(state, parsed)
    } catch {
      // ignore corrupt data
    }
  }

  // 持久化
  watch(
    () => ({ ...state }),
    (val) => {
      localStorage.setItem(key, JSON.stringify(val))
    },
    { deep: true }
  )
}

/**
 * 将单个 ref 同步到 localStorage。
 */
export function useLocalStorageRef<T>(key: string, ref: Ref<T>): void {
  const saved = localStorage.getItem(key)
  if (saved) {
    try {
      ref.value = JSON.parse(saved)
    } catch {
      // ignore
    }
  }

  watch(ref, (val) => {
    localStorage.setItem(key, JSON.stringify(val))
  })
}
