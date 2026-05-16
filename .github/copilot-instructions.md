# Copilot 指令 — Percussion

## 构建 / 运行 / 检查
- 本地跑 / 检查**都带 `--features dev`**（开 `bevy/dynamic_linking`）。
- lint 严格度：`cargo clippy --workspace --all-targets -- -D warnings`，warning 当错处理。
- 发布走 `cargo build --profile dist`（不是 `--release`）。

## ⚠️ Bevy 0.18 —— API 要查，别猜
模型脑里旧版本的记忆会写出错代码（已踩过：`WindowResolution` 在 0.18 只剩 `From<(u32, u32)>`）。写非平凡 Bevy 代码前查本地源码：

- 源码 `~/.cargo/registry/src/.../bevy_*-0.18.1/`，示例 `.../bevy-0.18.1/examples/`。
- `grep_search` 只覆盖 workspace，搜 registry 路径用 `execution_subagent` 跑 `rg`。
- 文档 <https://docs.rs/bevy/0.18.1/bevy/>，迁移指南 <https://bevy.org/learn/migration-guides/0-17-to-0-18/>。

## 已知踩坑
遇到 0.18 API 行为反常、Windows 构建 / 调试器怪事、`STATUS_DLL_NOT_FOUND` 之类 —— 先翻 [`doc/gotchas.md`](../doc/gotchas.md)。


