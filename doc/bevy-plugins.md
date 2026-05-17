# Bevy 0.18 社区插件清单

针对本项目（俯视 3D 空间 + 2D billboard sprite + ARPG）筛过一遍 Bevy 社区主流 plugin，按"几乎必备 / 推荐 / 看需要 / 暂时不可用"分类。**每条都核对了 crates.io / 上游仓库的版本兼容性**，时间点：2026-05。

> 选 plugin 的两个原则：
>
> 1. 它要解决一个**自己写代价不小**的问题（如 GPU 物理、aseprite 解析）。简单 30 行能搞定的事（如固定俯视相机）不挂插件。
> 2. 它要**跟上 Bevy 0.18**。卡在旧版本的 plugin 直接划掉，等就行。

## 兼容性汇总表

| Plugin | 最新版本 | Bevy 0.18 | 用途 |
|---|---|---|---|
| [`bevy_sprite3d`](https://crates.io/crates/bevy_sprite3d) | 8.0.0 | ✅ | 3D 空间里的 2D billboard sprite |
| [`avian3d`](https://crates.io/crates/avian3d) | 0.6.1 | ✅ | ECS-native 物理（首选） |
| [`bevy_rapier3d`](https://crates.io/crates/bevy_rapier3d) | 0.34.0 | ✅ | 物理（rapier 包装，替代选项） |
| [`bevy-inspector-egui`](https://crates.io/crates/bevy-inspector-egui) | 0.36.0 | ✅ | 运行时 inspector（dev-only） |
| [`bevy_egui`](https://crates.io/crates/bevy_egui) | 0.39.1 | ✅ | inspector 依赖；通用 egui 集成 |
| [`leafwing-input-manager`](https://crates.io/crates/leafwing-input-manager) | 0.20.0 | ✅ | 输入 → action 映射 |
| [`bevy_kira_audio`](https://crates.io/crates/bevy_kira_audio) | 0.25.0 | ✅ | 音频（channel / fade / 交叉切歌） |
| [`bevy_asset_loader`](https://crates.io/crates/bevy_asset_loader) | 0.26.0 | ✅ | Loading 进度条 / 多场景资源分组（小游戏可不用） |
| [`bevy_aseprite_ultra`](https://crates.io/crates/bevy_aseprite_ultra) | 0.8.2 | ✅ | 直接读 `.aseprite` 文件 |
| [`bevy_hanabi`](https://crates.io/crates/bevy_hanabi) | 0.18.0 | ✅ | GPU 粒子（2D/3D） |
| [`bevy_enoki`](https://crates.io/crates/bevy_enoki) | 0.6.0 | ✅ | CPU 2D 粒子（hanabi 的轻量替代） |
| [`bevy_tweening`](https://crates.io/crates/bevy_tweening) | 0.15.0 | ✅ | 补间动画 |
| [`bevy_ecs_tilemap`](https://crates.io/crates/bevy_ecs_tilemap) | 0.18.1 | ✅ | 2D tilemap 渲染（一 tile 一 entity） |
| [`bevy_yarnspinner`](https://crates.io/crates/bevy_yarnspinner) | 0.8.0 | ✅ | 对话脚本（Yarn Spinner Rust） |
| [`bevy_save`](https://crates.io/crates/bevy_save) | 3.0+4 | ❌（最新只到 Bevy 0.16） | 存档框架；等更新 |
| [`bevy_landmass`](https://crates.io/crates/bevy_landmass) | 0.11.1 | ❌（最新只到 Bevy 0.17） | 寻路 + 避障；等更新 |

---

## 几乎必备（核心玩法直接依赖）

### `bevy_sprite3d` 8.0.0

解决的问题：Bevy 自带 `Sprite` 是纯 2D pipeline，没法和 3D mesh / 光照 / 深度共存；自带 3D 也没有"sprite quad + billboard"的封装。这个 plugin 就是给"饥荒 / Don't Starve / Hades / Delver"这一类视觉风格量身做的。

要点：

- API 中心是 `Sprite3d` 组件 + `Sprite3dBuilder`。
- 内部缓存了 mesh 和 material，所以能批量用在 tilemap 之类的场景。
- 需要图片**先加载完**再 spawn —— 因为它要读图片的尺寸构 mesh quad。配合 `bevy_asset_loader` 的 loading state 用就刚好。

### `avian3d` 0.6.1（首选）

解决的问题：俯视 ARPG 必有的事 —— 角色碰墙、攻击命中、单位互相挤开。Bevy 内置不带物理。

为什么选 avian 不选 rapier：

- **ECS-native**：collider / rigidbody 都是 component，写法跟 Bevy 一致。
- **纯 Rust XPBD 实现**，作者就一个人，但社区活跃度和迁移速度都跟得上 Bevy 主线。
- 模块化插件架构，可以只加 spatial query 不加 dynamics，等等。

API 风格示例（Bevy 0.18）：

```rust
commands.spawn((
    RigidBody::Dynamic,
    Collider::cuboid(1.0, 1.0, 1.0),
    AngularVelocity(Vec3::new(2.5, 3.5, 1.5)),
));
```

### `bevy_rapier3d` 0.34.0（替代选项）

如果你需要 rapier 某些 avian 还没实现的特性（高级 joint、特定的 CCD 行为），用这个。

不推荐作为首选的原因：

- API 风格更像 rapier 而不是 Bevy，配置走 `RapierConfiguration` 之类的 resource，跟 ECS 哲学有点错位。
- 跟 Bevy 自己的 type（如 `Transform`）的 sync 走 plugin schedule 强耦合，行为不直观。

但它**稳定、文档多、被验证过**，新手出问题更容易找到解决方案。

### `bevy-inspector-egui` 0.36.0

dev-only 必装。运行时查看 entity tree、改 component 值、看 resource 状态。一人开发没有 QA，调试效率全靠它。

挂 `--features dev` 时启用，dist build 不带。典型挂法：

```rust
#[cfg(feature = "dev")]
app.add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::new());
```

**注意**：它会自动引入 `bevy_egui` 作为依赖。如果你以后想用 egui 写自己的调试面板，把 `bevy_egui` 显式加进 `Cargo.toml`（版本要和 inspector 对得上：0.36 → bevy_egui 0.39），避免被传递依赖锁死。

---

## 推荐（明显省工作量）

### `leafwing-input-manager` 0.20.0

解决的问题：Bevy 原生 input 查的是物理键码，要做"按 J 攻击 / J 可以重绑成 K / 手柄按钮也能触发同一个动作"这类需求，自己得写一大堆映射代码。

它给你的是：

- 定义一个 `Actionlike` enum（如 `Attack` / `Move` / `Dodge`）。
- 每个 entity 挂 `InputMap<MyAction>` 配键位 + `ActionState<MyAction>` 查状态。
- 同一个 action 可以绑多种输入（键盘 + 手柄 + UI 按钮），混用零成本。

ARPG 100% 会有重绑定需求，前期就用它，比后期改省事。

### `bevy_kira_audio` 0.25.0

解决的问题：Bevy 自带 audio 只能"播完拉倒"，没有 fade / 没有 channel / 没有交叉切 BGM。ARPG 要 BGM + 环境音 + 战斗音效分轨控制，自带的不够用。

⚠️ **互斥要点**：必须禁用 Bevy 默认的 `bevy_audio` 和 `vorbis` feature，不然两边都注册 `AudioSource` 类型会冲突。需要改 `bevy = { ..., default-features = false, features = [...] }`，手动列除 audio 外的所有 Bevy 默认 feature。

### `bevy_aseprite_ultra` 0.8.2

如果你打算用 Aseprite 做 sprite —— 这个 plugin 直接读 `.aseprite` 文件，自动切帧、读 tag 当动画状态。比自己写 atlas + frame 表方便得多。

特性：

- 支持 animation tag、frame duration、direction、ping-pong。
- 静态 slice 也能读（用 Aseprite 当 atlas 编辑器）。
- 热重载（需要开 Bevy 的 `file_watcher` feature）。

适用场景：用 Aseprite 画像素 sprite。如果你的素材全是 CC0 PNG / 已经切好的 atlas，不用引入这个。

---

## 看需要再加

### `bevy_asset_loader` 0.26.0

**小游戏（资源 < 100MB 全装内存）通常不需要它**。直接这么写就够：

```rust
#[derive(Resource)]
struct GameAssets {
    player: Handle<Image>,
    sword: Handle<Image>,
}

fn load_all(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(GameAssets {
        player: asset_server.load("player.png"),
        sword: asset_server.load("sword.png"),
    });
}
```

本地 SSD 读 50MB 资源也就 1-2 帧，反正第一帧渲染什么也看不出来，玩家无感。要么干脆 splash 黑屏 1 秒强切场景，根本不用查 loading state。

什么时候才值得装这个 plugin：

- 需要**真的 loading 进度条**（"Loading... 42%"），要每帧汇总 N 个 handle 的状态。
- 资源**按场景分组加载**（菜单一组 / 关卡一组 / boss 一组），多状态机来回切。
- 想用它的派生宏 + path 校验，省手写 `Handle` 字段。

API 中心是 `AssetCollection` 派生宏 + `LoadingState` 注入到 state 机。具体用法等真需要时翻 [crate 文档](https://docs.rs/bevy_asset_loader)。

### 粒子：`bevy_hanabi` 0.18.0 vs `bevy_enoki` 0.6.0

两个不同定位：

- **`bevy_hanabi`**：GPU 计算粒子，靠 compute shader。规模大（数万粒子）、效果复杂（轨迹、ribbon、力场）。适合火、魔法特效、爆炸。
- **`bevy_enoki`**：CPU 计算 + GPU instancing，配置走 `.ron` 文件，热重载方便，**支持 wasm 和移动端**，自带 editor。规模小但 ergonomic 好。

俯视 ARPG 早期：先用 enoki，简单粒子（伤害飞溅、技能命中）已经够。要做铺天盖地的法术 / 大范围 AOE 再考虑 hanabi。

### `bevy_tweening` 0.15.0

补间动画。UI 弹出、伤害数字弹跳、相机平滑跟随。

自己写 ease 函数 + 每帧 lerp 也就 20 行，但 chain 多个动画 / repeat 模式 / 完成事件这些它打包好了。

API 走 lens 模式：`TransformPositionLens { start, end }` 描述"插值的是 Transform 的 translation"。可以自定义 lens 改任意字段。

### `bevy_ecs_tilemap` 0.18.1

2D tilemap 渲染。每个 tile 一个 entity，chunk 内部合并 mesh 送 GPU。

你这游戏是**3D 空间**，不会直接用 2D tilemap。但如果地表用 tile 拼接（即使是 3D 平面贴 tile），它的"chunk + 共享 mesh" 思路可以参考甚至复用。或者干脆地面就是平铺的 sprite3d，每个 tile 一个 sprite3d entity（用它内部 mesh 缓存），用不上这个 plugin。

### `bevy_yarnspinner` 0.8.0

Yarn Spinner 的 Rust 移植。剧情对话脚本（变量、分支、跳转）。

纯刷怪 ARPG 可以不用。有 NPC 任务 / 剧情对白时再加 —— 比手写状态机管对话强多了。

---

## ⚠️ 暂时不可用（卡在旧版本，等更新）

### `bevy_save`

[GitHub 仓库](https://github.com/hankjordan/bevy_save)版本表显示：最新（v3.0+4）只支持 Bevy 0.16。

替代方案：

- 自己用 `serde` + Bevy 的 `Reflect` 序列化关键 component。
- 存档系统中后期再加，前期把可序列化的数据结构定下来就行。

### `bevy_landmass`

最新 v0.11.1 是 3 个月前发的，仓库 8 个月前才升到 Bevy 0.17，**未见 0.18 升级**。

替代方案：

- 早期：怪物直奔玩家（无寻路），不依赖这个。
- 中期：自己写网格 A*（如果地图是 tile 拼的，几十行就能搞）。
- 等开放世界 / 复杂 NPC 行为再回来看它有没有更新。

---

## 不推荐的方向

| 类别 | 为什么不挂 plugin |
|---|---|
| 相机 plugin（`bevy_panorbit_camera` 等） | 俯视固定视角相机就十几行代码：跟随玩家 + 固定 offset + 固定旋转。挂插件反而绑死了扩展（如战斗运镜、cutscene） |
| terrain heightmap plugin | 饥荒那种是地块拼接 + 装饰物，不是 heightmap terrain。地面直接用 mesh / sprite3d 拼，不需要 heightmap 生成 |
| ECS 状态机 plugin（`seldom_state` 等） | Bevy 0.16+ 自带 `States` + `SubStates` + `ComputedStates` 已经够用。AI / 角色行为状态机自己写 system 更直接 |
| timer / tween 替代品（`bevy_easings`） | 跟 `bevy_tweening` 重叠，选一个。tweening 更活跃 |

---

## 维护这份清单

新 Bevy 版本发布后，**优先检查这几个 critical plugin 的兼容性**：

1. `bevy_sprite3d` —— 没它整个视觉风格就没了
2. `avian3d` —— 没它没物理
3. `bevy-inspector-egui` —— 没它开发效率掉一截
4. `leafwing-input-manager` —— 没它输入系统要回退到原生

升 Bevy 版本前，先确认这 4 个都支持目标版本，再动手。其他 plugin 是锦上添花，可以等。
