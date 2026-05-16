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
