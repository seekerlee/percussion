//! 开发时相机控制器 —— 仅在 `debug_assertions` 启用时编译 / 注册。
//!
//! # 心智模型
//!
//! 这不是"游戏相机"的替代品，而是**开发期工具**：在 release 构建里整个
//! 文件不编译，相机保留 `lib.rs::spawn_camera` 给的固定俯视斜角姿态。
//! 在 debug 构建里挂上 [`bevy_panorbit_camera::PanOrbitCamera`] 组件，
//! 让原本固定的相机变得可拖动 / 缩放 / WASD 平移。
//!
//! # 操作
//!
//! - **鼠标左键拖动**：orbit（绕 focus 转）
//! - **鼠标右键拖动**：pan（沿相机本地右 / 上轴平移 focus）
//! - **滚轮**：zoom（拉近 / 推远 focus）
//! - **WASD**：在 XZ 平面按相机当前朝向平移 focus
//!   - `W` / `S` 沿相机水平投影前 / 后
//!   - `A` / `D` 沿相机水平投影左 / 右
//!
//! WASD 在 release 构建里不会被这个模块抢走（整个模块不编译），留给玩家
//! 输入。dev 构建里目前玩家移动跟相机平移会同时响应，等玩家输入逻辑迁
//! 移到别的键位（或限制到合适 state）后冲突自然消失。
//!
//! # 为什么不直接在 `spawn_camera` 里加 `PanOrbitCamera`
//!
//! `lib.rs::spawn_camera` 是 release 也跑的"游戏相机"。在它内部塞带 `cfg`
//! 的组件会让 release 路径里出现"为不存在的功能保留挂载点"的死代码，
//! 还得在 lib 顶层 import 这个 crate。改成 PostStartup 单独挂，dev 关注
//! 点完全留在 dev 模块里，lib.rs 不需要知道 `bevy_panorbit_camera` 存在。

use bevy::prelude::*;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use std::f32::consts::FRAC_PI_4;

/// 初始 pitch（弧度），对齐 `lib.rs::CAMERA_PITCH_DEG` 的 45°。
/// 不直接复用 lib 里的常量是为了让 dev_camera 自包含 —— 改这里不影响
/// release 构建里的相机姿态。
const INITIAL_PITCH: f32 = FRAC_PI_4;
/// 相机到 focus 的初始距离（米），对齐 `lib.rs::CAMERA_DISTANCE`。
const INITIAL_RADIUS: f32 = 12.0;
/// WASD pan 速度（米 / 秒）。
const PAN_SPEED: f32 = 8.0;

/// Dev camera 插件。仅 debug 构建里被 [`GamePlugin`](crate::GamePlugin) 注册。
pub struct DevCameraPlugin;

impl Plugin for DevCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PanOrbitCameraPlugin)
            .add_systems(PostStartup, attach_pan_orbit)
            .add_systems(Update, wasd_pan);
    }
}

/// 把 `PanOrbitCamera` 挂到 `spawn_camera` 在 Startup 里 spawn 的 Camera3d 上。
///
/// 用 PostStartup 是为了确保 Camera3d 已经存在（Startup 阶段顺序不强保证）。
/// `Without<PanOrbitCamera>` 过滤避免重复挂载（虽然 PostStartup 只跑一次，
/// 但这层保险让 system 即使被改成跑多次也安全）。
fn attach_pan_orbit(
    mut commands: Commands,
    q_camera: Query<Entity, (With<Camera3d>, Without<PanOrbitCamera>)>,
) {
    for entity in &q_camera {
        commands.entity(entity).insert(PanOrbitCamera {
            // 初始姿态对齐 lib.rs::spawn_camera：focus 在原点，俯视 45°，
            // 距离 12 米。挂上后 PanOrbitCamera 每帧覆写 Transform，所以
            // spawn_camera 里手算的 Transform 在 dev 构建里只用 1 帧。
            focus: Vec3::ZERO,
            radius: Some(INITIAL_RADIUS),
            yaw: Some(0.0),
            pitch: Some(INITIAL_PITCH),
            ..default()
        });
    }
}

/// WASD 在 XZ 平面按相机当前朝向平移 focus。
///
/// 朝向取自相机 `Transform` 的 `forward` / `right`，再把 Y 分量置零、
/// 重新归一化 —— 这样无论相机俯视多陡，WASD 都在地面上滑动，不会越走
/// 越高或越低。
///
/// 写到 `target_focus`（不是 `focus`）让 PanOrbitCamera 自带的平滑插值
/// 能介入。如果直接写 `focus`，会跟 `target_focus` 打架。
fn wasd_pan(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q_cam: Query<(&Transform, &mut PanOrbitCamera)>,
) {
    let mut input = Vec2::ZERO; // x = 右, y = 前
    if keys.pressed(KeyCode::KeyW) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        input.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        input.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        input.x += 1.0;
    }
    if input == Vec2::ZERO {
        return;
    }
    let input = input.normalize();
    let dt = time.delta_secs();

    for (transform, mut cam) in &mut q_cam {
        let forward = flatten_xz(*transform.forward());
        let right = flatten_xz(*transform.right());
        let delta = (forward * input.y + right * input.x) * PAN_SPEED * dt;
        cam.target_focus += delta;
    }
}

/// 把任意方向向量投影到 XZ 平面并归一化。
/// 用 `normalize_or_zero` 兜底极端情况（向量纯 Y）—— 那一帧不平移即可。
fn flatten_xz(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).normalize_or_zero()
}
