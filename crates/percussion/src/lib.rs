//! Percussion — game library.
//!
//! All game logic lives here as Bevy plugins so it can be exercised from
//! tests, alternative front-ends (e.g. a future WASM crate) or the thin
//! binary in `main.rs`.
//!
//! # 视觉技术路线
//!
//! 见 `doc/game-design.md` §15：**3D 世界 + 全 2D sprite + Y 轴 billboard**，
//! 参考《饥荒》（Don't Starve）。
//!
//! 坐标约定：Bevy 3D 默认 Y-up。本项目约定 **XZ 平面为地面**，**Y 为高度**。
//! 单位取 1.0 ≈ 1 米。
//!
//! 当前阶段只剩相机 + debug overlay，方便专注调通坐标轴 / 网格的可视化。
//! 占位 mesh（player / monster / ground / sun）已撤，等可视化敲定再回填。
//!
//! 资产组织：`unit` 提供角色身份的共享 marker；`stage` 提供舞台本身；
//! `player` 是受键盘驱动的一种 unit。

use avian3d::prelude::*;
use bevy::prelude::*;

pub mod app_state;
pub mod physics_layers;
pub mod projectile;
pub mod sprite_billboard;
pub mod stage;
pub mod unit;

// Dev-only 工具集（gizmo 网格、pan-orbit 相机等）。整段仅在 debug 构建里
// 编译；release / `--profile dist` 构建里 `dev/` 目录下所有文件都不会被
// 编译，零运行时开销。
#[cfg(debug_assertions)]
mod dev;

/// 相机俯视斜角（度）。具体最终值在 `doc/game-design.md` §17 待决，
/// 暂用 45°（饥荒视角的大致区间）。
const CAMERA_PITCH_DEG: f32 = 45.0;
/// 相机垂直 FOV（度）。Bevy 默认 45°；改小 = 视角更窄 / 物体更大（更
/// "长焦"）。30° 接近《饥荒》的轻度透视感，配合下方加大的
/// `CAMERA_DISTANCE` 维持原可见范围。
const CAMERA_FOV_DEG: f32 = 30.0;
/// 相机到焦点（原点）的距离，决定可见范围。
///
/// FOV 收窄到 30° 后，物体在画面里会放大约 1.5×（`tan(22.5°)/tan(15°)`），
/// 所以把距离从 12 拉到 18 抵消，保持原本的覆盖面积。这两个常量是
/// **一组**：单独改 FOV 或单独改距离都会改变可见范围。
const CAMERA_DISTANCE: f32 = 18.0;

/// Root plugin that wires the whole game together.
///
/// Add this to a fresh [`App`] and call `.run()` to start the game.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // Asset 路径走 Bevy 默认：base = `CARGO_MANIFEST_DIR`（cargo run / build
        // 时由 cargo 注入，指向本 bin crate 即 `crates/percussion/`）或
        // `current_exe().parent()`（不经 cargo 时）。因此 `assets/` 必须
        // 放在 bin crate 根下（`crates/percussion/assets/`），dist 部署时
        // assets/ 跟 exe 同目录。F5 / LLDB 直拉 exe 时通过 launch.json 的
        // `BEVY_ASSET_ROOT=${workspaceFolder}/crates/percussion` 把 base
        // 强制指回 bin crate 根。详见 doc/gotchas.md。
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Percussion".into(),
                resolution: (1280u32, 720u32).into(),
                // FifoRelaxed：准时帧按 vsync present（限帧到刷新率、不撕
                // 裂），但**错过 vsync 的迟到帧立刻 present**（容许那一帧撕
                // 裂）而不是等下一个 vsync。正好覆盖本项目踩到的两个坑：
                //
                // 1. `Fifo`（默认 `AutoVsync` 在 NVIDIA Vulkan 上实测 fallback
                //    到的就是 Fifo）：另一屏全屏视频时，DWM 合成抖动让单帧
                //    present 超 16.67ms，Fifo 强制等下一个 vsync = 33.3ms =
                //    周期性掉到 30fps。
                // 2. `Mailbox`：完全不限帧，focused 时 800+fps、GPU/CPU 满载
                //    烧电，对屏幕没意义（多余帧全丢）。
                //
                // FifoRelaxed 同时拿到 Fifo 的省电 + Mailbox 的抗 stall。代
                // 价：迟到帧那一瞬可能撕裂，但发生频率低、视觉几乎不可见；
                // G-Sync / FreeSync 显示器下完全不撕裂。
                //
                // 不用 `AutoVsync` 显式写死的原因：`AutoVsync` 的 fallback 顺
                // 序是 FifoRelaxed → Fifo，但驱动 / wgpu 版本组合下实际选哪
                // 个不可靠（实测会跑到 Fifo）。显式指定排除歧义。
                present_mode: bevy::window::PresentMode::FifoRelaxed,
                ..default()
            }),
            ..default()
        }))
        // 引擎层基础设施：物理在这里注册。Avian 不属于某个
        // 具体 plugin（stage / monster / bullet 都要用），放在最顶层避免
        // 重复注册和隐式 plugin 顺序依赖。
        .add_plugins(PhysicsPlugins::default())
        // AppState 状态机 + LoadingState 框架。**必须在下面任何调用
        // `configure_loading_state` 的领域 plugin（如 `PlayerPlugin`）之前**
        // add，否则 LoadingState 还没注册，配置请求会 panic。
        .add_plugins(app_state::AppStatePlugin)
        .add_plugins((
            unit::UnitPlugin,
            unit::movement::MovementPlugin,
            unit::hurtbox::HurtboxPlugin,
            unit::hitbox::HitboxPlugin,
            // Damage pipeline 的 3 个新阶段插件 —— UnitPlugin 已经在
            // `DamagePipeline` set 上拉好链；这里只是把 system 塞进对应 set。
            // 注册顺序无关（set 之间的 happens-before 由 chain() 决定），按
            // pipeline 流水顺序排只是为了阅读时容易看出"先 detect → 算 dmg →
            // 派 trigger → tick DoT → 判死"。
            unit::damage_calc::DamageCalcPlugin,
            unit::hit_triggers::HitTriggersPlugin,
            unit::burning::BurningPlugin,
            unit::skill::SkillPlugin,
            unit::skill_hitbox::SkillHitboxPlugin,
            projectile::ProjectilePlugin,
            sprite_billboard::BillboardPlugin,
            // bevy_sprite3d：3D 场景里的 2D sprite（Delver / 饥荒风）通
            // 用支持。`Sprite3d` 在 PostUpdate 的 bundle_builder system 里
            // 读 `Sprite.image` 尺寸 → 自动建 quad mesh + StandardMaterial；
            // mesh / material 资产内部缓存，多 entity 共享。详见
            // `unit/player.rs` 的 spawn 路径示例。
            bevy_sprite3d::prelude::Sprite3dPlugin,
            stage::StagePlugin,
            unit::player::PlayerPlugin,
            unit::dragon1::Dragon1Plugin,
        ))
        .add_systems(Startup, (spawn_camera, spawn_global_light))
        // 初始场景放到 `OnEnter(InGame)`：进到这个 state 时所有
        // `AssetCollection` 都保证已 insert，spawn 路径里 `Res<PlayerAssets>`
        // 拿到的 handle 全部已加载完成。相机 / 全局光留在 `Startup`——
        // 加载阶段也要有相机和光才能画 loading UI / debug 网格。
        .add_systems(OnEnter(app_state::AppState::InGame), spawn_initial_scene);

        // 只在 debug 构建（即非 --release / 非 --profile dist）里挂调试可视化 +
        // dev 相机控制器 + egui inspector。发布构建里 `dev` 模块都不会编译，
        // 零运行时开销。
        #[cfg(debug_assertions)]
        app.add_plugins((
            dev::grid::GridPlugin,
            dev::camera::CameraPlugin,
            // Note: avian 的 `PhysicsDebugPlugin` 走 Bevy `Gizmos`，玩家跑动时
            // 能看到 collider wireframe 周期性闪烁 (~500ms)。这是 Bevy 0.18
            // 上游的已知 issue #22438（gizmo asset event timing），在 0.19 以
            // PR #22964 修复。纯视觉，不影响物理 / 逻辑，暂不绕过。
            dev::physics_debug::PhysicsDebugPlugin,
            //dev::inspector::InspectorPlugin,
        ));

        // FPS overlay 来自 `bevy::dev_tools`，由 cargo feature `dev` 拉起
        // （`bevy/bevy_dev_tools`）。`dev::fps::FpsPlugin` 进一步要求
        // `debug_assertions`（整个 `dev` 模块的 gate），所以这里两个 cfg
        // 都要满足才注册 —— 防止 `--release --features dev` 这种组合下
        // 引用到不存在的模块。
        #[cfg(all(debug_assertions, feature = "dev"))]
        app.add_plugins(dev::fps::FpsPlugin);
    }
}

/// 俯视斜角 3D 相机：摆在 +Z 方向斜上方，看向原点。
///
/// `pitch` = 绕水平轴向下倾斜的角度（0° = 水平看，90° = 完全俯视）。
///
/// `Projection` 显式插入而不是走 `Camera3d` 自动补默认值——默认 FOV 是
/// 45°，本游戏想要 30°（见 `CAMERA_FOV_DEG` 注释）。其他字段（`near` /
/// `far` / `aspect_ratio`）保留 Bevy 默认。
fn spawn_camera(mut commands: Commands) {
    let pitch = CAMERA_PITCH_DEG.to_radians();
    let y = CAMERA_DISTANCE * pitch.sin();
    let z = CAMERA_DISTANCE * pitch.cos();
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: CAMERA_FOV_DEG.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, y, z).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Startup system：spawn 一盏**全局**方向光。
///
/// 当前是真正的"全局"——所有 stage 共用这一盏光。这跟 §8.5 的多 stage 同屏
/// 总览玩法长期会有冲突（每个 stage 应能有自己的氛围），但 per-stage 光照
/// 隔离要等多 camera + `RenderLayers` 架构落地时再做。现在先用一盏全局光
/// 把 `StandardMaterial` 点亮，避免一片漆黑。
///
/// 放这里而不是 `spawn_initial_stage` 里——它和具体 stage 没有耦合。
fn spawn_global_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 5_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// `OnEnter(AppState::InGame)` system：在原点 spawn 本游戏的首个 stage，然后在它里面 spawn 玩家。
///
/// 这是**游戏启动策略**（决定开局长什么样），不是某个 plugin 的能力 ——
/// 所以归 `GamePlugin` 管。`StagePlugin` / `PlayerPlugin` 只提供 spawn API，
/// 由谁、何时、何地、用什么尺寸调，是调用方的决策。
///
/// 资产需求通过 [`unit::player::PlayerAssets`] 资源注入：能走到这里说明
/// `LoadingState` 已经把所有 `AssetCollection` 填好并 insert 为 Resource，
/// 不需要再绕道 `AssetServer` 手工 `load(...)`。
fn spawn_initial_scene(
    mut commands: Commands,
    player_assets: Res<unit::player::PlayerAssets>,
    dragon1_assets: Res<unit::dragon1::Dragon1Assets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 初始 stage 尺寸（地面 X × Z 全长，米）—— 本游戏的开局关卡决策，
    // 不是 stage 这个能力的固有属性。
    let size = Vec2::new(20.0, 15.0);
    // 盒子净空高度（米）：物理屏障 + 视觉罩的顶面就在这个高度。保守值，
    // 等子弹弹道做出来再调。
    let height = 10.0;
    let stage_entity = stage::spawn_stage(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::ZERO,
        size,
        height,
    );

    // 玩家在 stage 中央上方 5 米处生成，靠重力落下 —— 可以用肉眼验证
    // 物理接触、撞 bounds 屏障的反馈都正常工作。
    unit::player::spawn_player(
        &mut commands,
        &player_assets,
        stage_entity,
        Vec3::new(0.0, 5.0, 0.0),
    );

    // Dragon1 占位 —— 在玩家旁边落下，验证 sprite 加载 + Unit 共享通路。
    // 之后接 AI 时这个 spawn 调用会被 wave / spawner 系统替代。
    unit::dragon1::spawn_dragon1(
        &mut commands,
        &dragon1_assets,
        stage_entity,
        Vec3::new(3.0, 5.0, 0.0),
    );
}
