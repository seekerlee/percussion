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
pub mod player;
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
/// 相机到焦点（原点）的距离，决定可见范围。
const CAMERA_DISTANCE: f32 = 12.0;

/// Root plugin that wires the whole game together.
///
/// Add this to a fresh [`App`] and call `.run()` to start the game.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Percussion".into(),
                        resolution: (1280u32, 720u32).into(),
                        ..default()
                    }),
                    ..default()
                })
                // Bevy 把 asset 文件 dir 拼成 `<base_path>/<file_path>`，
                // 其中 base_path 优先取 `BEVY_ASSET_ROOT` env，否则取
                // `CARGO_MANIFEST_DIR` env，否则取 exe 所在目录。
                //
                // 本项目 assets/ 在 workspace root，但：
                // - `cargo run` 时 CARGO_MANIFEST_DIR = `crates/percussion/`
                //   （binary crate 的 manifest 目录，不是 workspace root）
                // - LLDB 直接拉 exe 时 base_path = `target/debug/`
                //
                // 两种 dev 启动方式下 base_path 恰好都比 workspace root
                // 深两级，所以 file_path 设 "../../assets" 在两种情况下都
                // 指向 workspace root 的 assets/。release 部署假设 assets/
                // 跟 exe 同目录，走默认 "assets"。
                .set(AssetPlugin {
                    file_path: if cfg!(debug_assertions) {
                        "../../assets".to_string()
                    } else {
                        "assets".to_string()
                    },
                    ..default()
                }),
        )
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
            sprite_billboard::BillboardPlugin,
            stage::StagePlugin,
            player::PlayerPlugin,
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
            dev::inspector::InspectorPlugin,
        ));
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

/// `OnEnter(AppState::InGame)` system：在原点 spawn 本游戏的首个 stage，然后在它里面 spawn 玩家。
///
/// 这是**游戏启动策略**（决定开局长什么样），不是某个 plugin 的能力 ——
/// 所以归 `GamePlugin` 管。`StagePlugin` / `PlayerPlugin` 只提供 spawn API，
/// 由谁、何时、何地、用什么尺寸调，是调用方的决策。
///
/// 资产需求通过 [`player::PlayerAssets`] 资源注入：能走到这里说明
/// `LoadingState` 已经把所有 `AssetCollection` 填好并 insert 为 Resource，
/// 不需要再绕道 `AssetServer` 手工 `load(...)`。
fn spawn_initial_scene(
    mut commands: Commands,
    player_assets: Res<player::PlayerAssets>,
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
    player::spawn_player(
        &mut commands,
        &player_assets,
        &mut meshes,
        &mut materials,
        stage_entity,
        Vec3::new(0.0, 5.0, 0.0),
    );
}
