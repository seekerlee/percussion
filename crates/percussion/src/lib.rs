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

use avian3d::prelude::*;
use bevy::prelude::*;

pub mod stage;

// 仅 debug 构建编译；发布构建里整段不存在，零运行时开销。
#[cfg(debug_assertions)]
mod debug;

// Dev 相机控制器（pan-orbit + WASD），同样仅 debug 构建。详见
// `dev_camera.rs` 模块文档。
#[cfg(debug_assertions)]
mod dev_camera;

/// 相机俯视斜角（度）。具体最终值在 `doc/game-design.md` §17 待决，
/// 暂用 45°（饥荒视角的大致区间）。
const CAMERA_PITCH_DEG: f32 = 45.0;
/// 相机到焦点（原点）的距离，决定可见范围。
const CAMERA_DISTANCE: f32 = 12.0;

/// Root plugin that wires the whole game together.
///
/// Add this to a fresh [`App`] and call `.run()` to start the game.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Percussion".into(),
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        // 引擎层基础设施：物理在这里注册。Avian 不属于某个
        // 具体 plugin（stage / monster / bullet 都要用），放在最顶层避免
        // 重复注册和隐式 plugin 顺序依赖。
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(stage::StagePlugin)
        .add_systems(
            Startup,
            (spawn_camera, spawn_global_light, spawn_initial_stage),
        );

        // 只在 debug 构建（即非 --release / 非 --profile dist）里挂调试可视化 +
        // dev 相机控制器。发布构建里两个模块都不会编译，零运行时开销。
        #[cfg(debug_assertions)]
        app.add_plugins((debug::DebugOverlayPlugin, dev_camera::DevCameraPlugin));
    }
}

/// 俯视斜角 3D 相机：摆在 +Z 方向斜上方，看向原点。
///
/// `pitch` = 绕水平轴向下倾斜的角度（0° = 水平看，90° = 完全俯视）。
fn spawn_camera(mut commands: Commands) {
    let pitch = CAMERA_PITCH_DEG.to_radians();
    let y = CAMERA_DISTANCE * pitch.sin();
    let z = CAMERA_DISTANCE * pitch.cos();
    commands.spawn((
        Camera3d::default(),
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

/// Startup system：在原点 spawn 本游戏的首个 stage。
///
/// 这是**游戏启动策略**（决定开局长什么样），不是 stage 能力本身 ——
/// 所以归 `GamePlugin` 管，不归 `StagePlugin`。`StagePlugin` 只提供
/// [`stage::spawn_stage`] 这个 API，由谁、何时、何地、用什么尺寸调，
/// 是调用方的决策。
fn spawn_initial_stage(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 初始 stage 尺寸（地面 X × Z 全长，米）—— 本游戏的开局关卡决策，
    // 不是 stage 这个能力的固有属性。
    let size = Vec2::new(20.0, 15.0);
    // 逻辑顶高（米）：飞过这个高度的子弹算"出界"。保守值，等子弹机制做出来再调。
    let height = 10.0;
    stage::spawn_stage(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::ZERO,
        size,
        height,
    );
}
