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

use bevy::prelude::*;

// 仅 debug 构建编译；发布构建里整段不存在，零运行时开销。
#[cfg(debug_assertions)]
mod debug;

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
        .add_systems(Startup, spawn_camera);

        // 只在 debug 构建（即非 --release / 非 --profile dist）里挂调试可视化。
        // 发布构建里整个模块都不会编译，零运行时开销。
        #[cfg(debug_assertions)]
        app.add_plugins(debug::DebugOverlayPlugin);
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
