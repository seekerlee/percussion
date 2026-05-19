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

### Bevy 找 `assets/` 的目录不是 CWD —— multi-crate workspace + F5 调试双重背刺

**症状**：`AssetServer` 报 `ERROR Path not found: <somewhere>\assets\<file>`。CWD 明明对，文件也确实存在，但 Bevy 找的"somewhere"不是 CWD。常见错位：

- F5 / LLDB 调试启动：`<project>/target/<profile>/assets/<file>`
- `cargo run` / `cargo run --release` 启动 multi-crate workspace 的 bin：`<project>/crates/<bin>/assets/<file>`

**原因**：Bevy 0.18 `bevy_asset` 的 base path 优先级（`bevy_asset/src/io/file/mod.rs::get_base_path`）：

1. `BEVY_ASSET_ROOT` 环境变量
2. `CARGO_MANIFEST_DIR` 环境变量
3. fallback：`current_exe().parent()`

它**不看 CWD**。然后跟 `AssetPlugin::file_path`（默认 `"assets"`）join。所以：

- `cargo run` / `cargo run --release` 时 cargo 注入 `CARGO_MANIFEST_DIR` = 当前 package 的 manifest 目录。multi-crate workspace 里二进制 crate 在 `crates/<name>/`，于是 Bevy 找 `crates/<name>/assets/`。
- F5 / LLDB 直接 spawn exe，不经过 cargo，`CARGO_MANIFEST_DIR` 没设，fallback 到 exe 旁边 = `target/<profile>/assets/`。

**修复**：跟 Bevy 默认行为对齐 —— 把 `assets/` 放在 bin crate 根下（即 `crates/percussion/assets/`），代码里**不**自定义 `AssetPlugin::file_path`。

| 启动方式 | base path | `+ "assets"` | 结果 |
|---|---|---|---|
| `cargo run` (dev / release) | `crates/percussion/` | `crates/percussion/assets/` | ✅ |
| F5 / LLDB | `target/<profile>/` → 用 env 改成 `crates/percussion/` | `crates/percussion/assets/` | ✅ |
| dist 部署 exe | exe 旁边 | `<exe_dir>/assets/` | ✅ |

F5 / LLDB 那条用 `.vscode/launch.json` 里的 env 把 base 强制指回 bin crate 根：

```jsonc
"env": {
    "BEVY_ASSET_ROOT": "${workspaceFolder}/crates/percussion"
}
```

只有调试器配置吃这个 env，CLI 走 cargo 不受影响，部署后的 exe 也读不到（cargo 没参与启动），不污染。

**走过的弯路**（别再踩）：

- ❌ `AssetPlugin::file_path = "../../assets"` + `cfg(debug_assertions)`：依赖"两种 dev 启动方式 base path 恰好都比 workspace root 深两级"的巧合。`cargo run --release` 一来就翻 ——`--release` 关掉 `debug_assertions` 走 release 分支 `"assets"`，base 还是 `crates/percussion/`，依然找不到。给 release 分支加 `../../assets` 又会污染 dist 部署。
- ❌ `cargo run --release --features dev` 试图保住 `debug_assertions`：feature 不会改 profile 的 `debug-assertions` 设置，这条 workaround 不成立。
- ❌ `.cargo/config.toml` 里 `BEVY_ASSET_ROOT = { value = "..", relative = true }`：值是相对 config.toml 所在目录（workspace root）展开的，写 `".."` 会跑到 workspace **外**。正确写法是 `"."`，但既然现在 assets 在 bin crate 里，根本不需要这条 env。


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

### 调试器下运行还是卡 —— CodeLLDB 默认 PDB reader 太慢

**症状**：`_NO_DEBUG_HEAP=1` 已经配上、调试堆开销也排除，但 F5 启动**还是**卡。`cargo run` 同代码丝滑、F5 同代码 CPU 满载 GPU 闲、窗口几乎不响应 —— 跟 GPU 渲染问题的"GPU 满 CPU 闲"完全相反，是判别 debugger overhead 的关键信号。

**原因**：CodeLLDB 在 Windows 上默认用 **DIA-based PDB reader**（微软的 COM 组件 DbgHelp/DIA SDK），完整但慢。Bevy 开 `dynamic_linking` 后 exe 拽着几十个 DLL（`bevy_*`、`wgpu_*`、`naga_*`、`std-*`…），调试器启动时挨个 PDB 扫一遍 → CPU 长时间堆积 → 主线程拿不到调度 → 窗口看上去卡死。

**修复**：`.vscode/settings.json` 加一行：

```jsonc
"lldb.useNativePDBReader": true
```

切到 CodeLLDB 自带的 native PDB reader。**改完要 Reload Window 才生效**（settings 是启动时读的）。

bisect 实测：单这一项就够，下面这些都不是必要条件，加了也只是杯水车薪 —— 不要无脑全开：

- ❌ `lldb.evaluateForHovers: false`：只在编辑器悬停变量时触发，跟"启动卡死"无关
- ❌ `lldb.commandCompletions: false`：只在 DEBUG CONSOLE 输入命令时触发，无关
- ❌ launch.json `initCommands: ["settings set target.preload-symbols false"]`：`useNativePDBReader` 开了之后 preload 已经够快，再延迟也没收益

**跟 `_NO_DEBUG_HEAP` 是独立两件事**：一个是 OS-level 堆开销，一个是 lldb-level PDB 加载开销。Bevy 项目两个都要配。

**诊断顺序**（下次 F5 卡死时按这个排查）：
1. 看 CPU / GPU 负载 —— CPU 满 GPU 闲 → debugger 锅；GPU 满 CPU 闲 → 引擎 / shader 锅。先分清楚再修。
2. 检查 `_NO_DEBUG_HEAP=1` 在不在 `launch.json` 的 `env` 里
3. 检查 `lldb.useNativePDBReader: true` 在不在 `settings.json` 里


## Bevy ECS / 调度

### B0001 是 SystemParam 的"声明访问"在打架 —— 跟运行时实际用没用无关

**症状**：app 启动期 panic，`error[B0001]: Query<...> in system <X> accesses component(s) Foo in a way that conflicts with a previous system parameter`。

**原因**：Bevy scheduler 在 system 注册时，**静态、保守、聚合**地扫描 system 的所有参数，凡是带 `Query` / `Res` / `ResMut` 的（包括嵌在 `#[derive(SystemParam)] struct` 里的字段）都纳入"声明访问集"。两个集合在某个组件 + 某个实体子集上越权（`&mut` × `&` 或 `&mut` × `&mut`）就拒绝。**不看运行时实际是否同时访问。**

第三方 SystemParam（avian / leafwing-input / egui / replicon …）每个都是一个"自带嫁妆"的结构体：你写 `mover: MoveAndSlide` 等于声明了它内部 `spatial_query: SpatialQuery` 那条线一路下去所有 `Query<&Position, ..>` 之类的访问。

**诊断流程**：

1. panic 信息抓"previous system parameter" 的名字
2. `rg "pub struct <名字>" -A 30` 找它的 `#[derive(SystemParam)]` 字段
3. 字段里每个 `Query<...>` / `Res<...>` 是已花掉的 budget，**整套**继承到你的 system
4. 你自己 query 在剩下空间里挑

普遍适用于**任何**第三方 SystemParam，不局限于 avian。这是写"组合式"游戏系统的根本边界感。

### avian: `MoveAndSlide` 自己更新位置走 `&mut Transform`，不要碰 `&mut Position`

[`MoveAndSlide`] 内部 `spatial_query: SpatialQuery` 字段递归过去声明了 `Query<&Position, ..>`（全实体只读）。外层 query 写 `&mut Position, With<Body>` 跟它在 Body 子集上 read/write 冲突 → 上面那条 B0001。

avian 官方 `examples/move_and_slide_3d.rs` / `examples/kinematic_character_3d/` 的标准写法是 query `&mut Transform`：

```rust
Query<(Entity, &Collider, &mut Transform, ...), With<...>>
// ...
transform.translation = out.position;  // 不写 Position
```

`PhysicsSystems::Prepare` 阶段 avian 会做 `Transform → Position` 同步，所以"写 Transform.translation"等价于"设置 avian 位置"。Transform 不属于 avian 任何 SystemParam 的查询范围，零冲突。

`Rotation` / `Collider` 是只读访问，自己 query 里挂也行（read × read 不冲突）。**唯一**禁忌就是 `&mut Position`。

### avian 0.18 / 0.6 把 `PhysicsSet` 改叫 `PhysicsSystems`

旧名仍能 compile，但带 `deprecated` 警告。`cargo clippy -- -D warnings` 直接 fail。
迁移就是字面意思 `PhysicsSet::Prepare` → `PhysicsSystems::Prepare`。变体名不变。

