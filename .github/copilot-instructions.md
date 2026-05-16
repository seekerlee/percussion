# Copilot 指令 — Percussion

# Copilot 项目指令

## 用户背景
- 我是一个工程师经理，写过 javascript/typescript，Java，C#，C++
- 不喜欢 OOP，喜欢函数式编程
- 主要做过 web, saas 服务，云计算，后端基础设施等项目
- 用过 RDBMS，NoSQL，Kusto 等
- 没有开发过游戏，懂得 ECS

## 项目背景
- 这个游戏只有我 1 人开发，我的角色是程序
- 没有专职美术 / 音效 / 游戏设计师 / QA
- 美术 / 音效 / 关卡设计的能力都是稀缺资源，方案优先考虑：
  - 用代码 / 参数生成代替手工资源（程序化建模、tile 复用、shader 而非贴图）
  - 用免费 / CC0 资源代替自制（音效、贴图、字体）
  - 减少素材种类，强调复用（同一组 sprite 多角色复用、调色板换装）
- 游戏设计 doc 在 [`doc/game-design.md`](../doc/game-design.md)，思维导图在 [`doc/game-design-mindmap.md`](../doc/game-design-mindmap.md)。讨论玩法 / 视觉决策前先翻一下，doc 已确定的事不再重新讨论。

## 用户工作风格
- 先理解原理再动手，解释清楚"为什么"比直接给代码更重要
- 重视架构整洁，关注职责分离和可维护性
- 渐进式推进，每次只做一件事，步步验证
- 追根究底，会深入追问概念和边界情况，回答要有深度
- 已知会做且后改成本高的事（如核心数据结构、跨层边界、命名规范），一开始就做好；猜测性的扩展点（interface、配置项、预留参数）不要提前加
- 可以适当介绍下 best practice，但不强求一定要遵守，让用户选择
- 不用着急写代码，给方案。先把哲学问题讨论清楚。哲学问题就是这个东西的本质是什么，他应该负责什么。

## 交流
- 变量/函数/类型用英文命名
- 讲新概念时，先说"它解决什么问题"（why），再说"怎么做到的"（how）。不要一上来就讲实现细节和性能优化
- 用户问问题就是在问问题，不是在质疑现状、不是在暗示要改。如果有明确的想法或修改意图，会直接说出来。所以收到问题时只回答问题本身），不要顺手提出修改建议、不要追问"要不要改"，除非用户明确要求方案

## 工作流程
- 改代码前先说方案，等用户确认后再动手
- sprite 帧数、尺寸等资源信息让用户确认，不要猜
- 涉及多文件改动时，附上"职责分离验证"表格：列出每个 system/handler 做了什么、不做什么，确认引擎层无领域知识、逻辑层无引擎细节
- 修改已有文件时，先读完整上下文，找到文件已有的模式，新代码必须顺着已有模式写，除非不合适，不要绕开已有抽象手动实现

## 代码风格
- 命名要贴近本质，如果要做比喻（比如 world），一定要用户确认。不管是文件名，还是变量名。宁愿啰嗦一点，也不要模糊不清有歧义。
- 考虑多加注释，特别是 Bevy / Rust 独有的东西，或者是游戏开发的特点的东西
- 代码风格参考 Bevy 官方示例和 Rust 官方示例，保持一致
- API 设计上尽量让误用不可能发生，而不是靠文档叮嘱"别忘了调 X"。
- Plugin 默认放在独立文件里（一个 plugin 一个 module 文件），除非用户明确说就地写。

## Git
- commit message 用中文

## 构建 / 运行 / 检查
- 本地跑 / 检查**都带 `--features dev`**（开 `bevy/dynamic_linking`）。
- lint 严格度：`cargo clippy --workspace --all-targets -- -D warnings`，warning 当错处理。
- 发布走 `cargo build --profile dist`（不是 `--release`）。

## ⚠️ Bevy 0.18 —— API 要查，别猜
模型脑里旧版本（0.15）的记忆会写出错代码（已踩过：`WindowResolution` 在 0.18 只剩 `From<(u32, u32)>`）。写非平凡 Bevy 代码前查本地源码：

- 源码 `~/.cargo/registry/src/.../bevy_*-0.18.1/`，示例 `.../bevy-0.18.1/examples/`。
- `grep_search` 只覆盖 workspace，搜 registry 路径用 `execution_subagent` 跑 `rg`。
- 文档 <https://docs.rs/bevy/0.18.1/bevy/>，迁移指南 <https://bevy.org/learn/migration-guides/0-17-to-0-18/>。

## 已知踩坑
遇到 0.18 API 行为反常、Windows 构建 / 调试器怪事、`STATUS_DLL_NOT_FOUND` 之类 —— 先翻 [`doc/gotchas.md`](../doc/gotchas.md)。


