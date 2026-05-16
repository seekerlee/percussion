//! Percussion — game library.
//!
//! All game logic lives here as Bevy plugins so it can be exercised from
//! tests, alternative front-ends (e.g. a future WASM crate) or the thin
//! binary in `main.rs`.

use bevy::prelude::*;

// 仅 debug 构建编译；发布构建里整段不存在，零运行时开销。
#[cfg(debug_assertions)]
mod debug;

// --- 占位资源配置 ---------------------------------------------------------
// 程序设计阶段没有美术，统一用 `Sprite::from_color` 画纯色方块占位。
// 等美术到位后，只需替换这里的颜色 / 尺寸常量和对应的 spawn 函数。

const PLAYER_COLOR: Color = Color::srgb(0.2, 0.8, 0.3);
const PLAYER_SIZE: Vec2 = Vec2::new(24.0, 32.0);

const MONSTER_COLOR: Color = Color::srgb(0.9, 0.2, 0.2);
const MONSTER_SIZE: Vec2 = Vec2::splat(28.0);

/// 玩家标记组件。
#[derive(Component)]
pub struct Player;

/// 怪物标记组件。
#[derive(Component)]
pub struct Monster;

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
        .add_systems(Startup, (spawn_camera, spawn_player, spawn_monsters));

        // 只在 debug 构建（即非 --release / 非 --profile dist）里挂调试可视化。
        // 发布构建里整个模块都不会编译，零运行时开销。
        #[cfg(debug_assertions)]
        app.add_plugins(debug::DebugOverlayPlugin);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Player,
        Sprite::from_color(PLAYER_COLOR, PLAYER_SIZE),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}

fn spawn_monsters(mut commands: Commands) {
    // 几只占位怪物，分散摆放，方便后续接入 AI / 战斗逻辑时一眼能看到。
    for (x, y) in [(-200.0, 80.0), (200.0, 80.0), (0.0, -120.0)] {
        commands.spawn((
            Monster,
            Sprite::from_color(MONSTER_COLOR, MONSTER_SIZE),
            Transform::from_xyz(x, y, 0.0),
        ));
    }
}
