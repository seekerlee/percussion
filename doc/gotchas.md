# 踩过的坑

写代码 / 调 build 之前，遇到下面这些场景先来翻一下。

## Bevy 0.18 API

### `WindowResolution` 没有 `From<(f32, f32)>`

0.18 只实现了 `From<(u32, u32)>` / `From<[u32; 2]>` / `From<UVec2>`。`From<(f32, f32)>` 已经移除。

```rust
// ❌
resolution: (1280.0, 720.0).into(),
// ✅
resolution: (1280u32, 720u32).into(),
```

旧版本的肌肉记忆/模型记忆会直接写出第一种。用整数字面量。

## Windows 构建 / 调试

### `bevy/dynamic_linking` 拉 exe 直接挂 `STATUS_DLL_NOT_FOUND`

**症状**：F5 / 双击 exe / 任何不走 `cargo run` 的方式启动，进程瞬间退出，退出码 `-1073741515` / `0xC0000135`。

**原因**：开 `dev` feature 后 exe 依赖**两个**运行时 DLL：

- `bevy_dylib-<hash>.dll` 在 `target/debug/deps/`
- `std-<hash>.dll` 在 Rust 工具链的 `bin/`（如 `~/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/`）

只有 `cargo run` 会自动把这两个目录注进子进程的 `PATH`。其他启动方式 —— `cppvsdbg`、双击、**CodeLLDB 的 `cargo` 块**（它跑的是 `cargo build`，然后直接拉 exe）—— 都拿不到这两条路径。

**为什么不用 launch.json 的 `env: { "PATH": ... }`**：在 Windows 上**不可靠**，实测就算写绝对路径，DLL 也加载不到。原因不明，可能跟 VS Code 变量展开或 CodeLLDB 在 Windows 上的 env 传递有关 —— 不要再花时间在这条路上。

**当前方案（已在 `.vscode/` 里就位）**：

1. `tasks.json` 的任务 **`dev: build & stage DLLs`** 通过 `dependsOn` 走 `cargo: build (dev, dynamic linking)`，build 完把 `std-*.dll` 和 `bevy_dylib-*.dll` 复制到 `target/debug/percussion.exe` 旁边。
2. `launch.json` 的 dev 配置用 `preLaunchTask` + `program`（不用 `cargo` 块、不用 `env` 块）。

Windows DLL 搜索一定先看 exe 自己的目录，所以这条免疫 PATH / 变量展开的所有怪事。

**维护点**：工具链 triple `stable-x86_64-pc-windows-msvc` 在 staging 任务里硬编码。`rust-toolchain.toml` 的 channel / target 一旦改了，这里**一起改**。

### 调试器下运行卡到无响应 —— Windows "调试堆"在背刺

**症状**：`cargo run` 流畅，**F5 启动调试**就极卡。窗口几秒才出来，鼠标拖不动、关不掉，结束进程要等几分钟。CPU 没满，看着像死锁但其实没死，纯粹响应不过来。

**原因**：跟 LLDB / Rust / Bevy 都没关系。**Windows 内核** 自带的行为：进程在调试器下启动时（`CreateProcess` 带 `DEBUG_PROCESS` 标志），系统**自动**把默认堆切换成"调试堆"。任何调试器都会触发 —— LLDB、Visual Studio、WinDbg 一视同仁。

调试堆为了帮 C/C++ 抓堆 bug 偷偷做的事：

| 操作 | 实际开销 |
|---|---|
| `alloc(N)` | 实际分配 `N + guard`，前后填 `0xABABABAB` 检测越界 |
| `free(p)` | 检查 guard、填 `0xFEEEFEEE`、做全堆一致性扫描 |
| 始终维护 | 一个全局链表追踪所有 live allocations |

**速度税：alloc/free 慢 10–100 倍**，free 尤其慢。Bevy 这种每帧大量小分配的引擎（ECS 存储 realloc、Text2d 重排版、mesh 顶点缓冲…）会被严重放大，主线程预算溢出 → Windows 消息泵收不到调度 → 看上去窗口"无响应"。

**修复**：在 `launch.json` 的 dev 配置里注入环境变量：

```jsonc
"env": {
    "_NO_DEBUG_HEAP": "1"
}
```

这是 Windows 自己埋的 opt-out 开关：NT 加载器在进程启动时检查这个变量，**有就跳过调试堆切换**。下划线前缀是变量名的一部分，不能漏。环境变量必须在 `CreateProcess` 之前就有 —— CodeLLDB 的 `env` 块在 spawn 子进程时注入，时机正好。

**副作用对 Rust 项目而言为零**：调试堆原本能抓"堆越界 / use-after-free / double-free"，这些 bug Rust 借用检查器在编译期就消灭了，根本不存在。安全网失效不影响我们。

**别在哪里设这个变量**：
- ❌ 系统全局环境变量 / 用户环境变量：会影响所有调试进程，过头了
- ❌ `tasks.json` 的 task：`cargo run` 没问题，不需要
- ✅ `launch.json` 里**单条调试配置** 的 `env` 块：作用域刚好
