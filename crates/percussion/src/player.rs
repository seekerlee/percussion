//! Player —— 受键盘输入驱动的 [`Unit`](crate::unit::Unit)。
//!
//! # 与 Unit 的关系
//!
//! [`Player`] 是 [`Unit`] 的一种特化身份："这个 unit 被键盘驱动"。通过
//! `#[require(Unit)]` 声明，spawn `Player` 时 Bevy 自动补上 `Unit` marker。
//! 这样：
//!
//! - 通用 unit 机制（`With<Unit>` 的 system）自动覆盖玩家，不会漏。
//! - Player 专属 system 用 `With<Player>` filter，跟 AI / 敌人系统正交。
//!
//! # 当前只是占位
//!
//! 视觉是一个亮黄色立方体；等 sprite billboard 视觉敲定（见
//! `doc/game-design.md` §15）再换成真正的角色实体。物理参数（碰撞盒、
//! 移动速度）也只是 prototype 值。

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::unit::{DamageMessage, Dead, Health, Unit};

/// 玩家立方体边长（米）。
const PLAYER_SIZE: f32 = 1.0;
/// 玩家平移速度（米/秒）。
const PLAYER_SPEED: f32 = 5.0;
/// 玩家初始最大生命值。数值是 prototype 阶段的占位，等战斗公式立起来再调。
const PLAYER_MAX_HEALTH: f32 = 100.0;

/// 玩家标记。
///
/// `#[require(...)]` 是 Bevy 0.15+ 的 required components 机制：spawn `Player`
/// 时 Bevy 自动挂上这些依赖组件 —— 语义上等于"`Player` 是一种 `Unit`，
/// 且无需手写的生命值初始为满血"。实现上是组合而非继承：组件都挂在
/// 同一 entity 上。
#[derive(Component, Debug, Default)]
#[require(Unit, Health = Health::new(PLAYER_MAX_HEALTH))]
pub struct Player;

/// Player 插件 —— 注册键盘移动 system，以及 debug build 下的调试快捷键。
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, player_movement);

        // debug 调试快捷键仅 debug 构建编译，release / dist 零运行开销。
        // 现阶段还没有实际的伤害源（敌人 / 陷阱），这两个键位用来手动
        // 验证 Health / Dead / 复活 的路径是否走通。
        #[cfg(debug_assertions)]
        app.add_systems(
            Update,
            (debug_damage_player_on_space, debug_revive_player_on_r),
        );
    }
}

/// 在指定 stage 下 spawn 玩家，返回 player entity。
///
/// 玩家作为 `parent_stage` 的子实体（通过 [`ChildOf`] relationship），
/// 这样 stage despawn 时玩家自动连带销毁；`local_pos` 是相对 stage 局部
/// 坐标系的初始位置。
///
/// # 参数
///
/// - `parent_stage`：[`spawn_stage`](crate::stage::spawn_stage) 返回的根 entity
/// - `local_pos`：stage 局部坐标系下的初始位置（Y > 0 让玩家从空中落下）
pub fn spawn_player(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    parent_stage: Entity,
    local_pos: Vec3,
) -> Entity {
    let mesh = meshes.add(Cuboid::from_size(Vec3::splat(PLAYER_SIZE)));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.00, 0.95, 0.36),
        ..default()
    });

    commands
        .spawn((
            Player,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(local_pos),
            // Dynamic 刚体：靠重力落到地面，靠 stage 物理屏障挡 XZ 移动。
            RigidBody::Dynamic,
            Collider::cuboid(PLAYER_SIZE, PLAYER_SIZE, PLAYER_SIZE),
            // 防止被撞翻滚 —— 俯视斜角游戏角色应保持站立。
            LockedAxes::ROTATION_LOCKED,
            // Bevy 0.18 relationship API：把自己挂成 parent_stage 的子实体。
            ChildOf(parent_stage),
        ))
        .id()
}

/// 方向键移动玩家：每帧根据按键设置 X/Z 方向线速度，Y 由重力管。
///
/// 朝向约定：相机在 +Y +Z 看向原点（见 `lib.rs::spawn_camera`），所以屏幕
/// 上"远端 = -Z"。WASD 留给 dev 相机（见 `dev_camera.rs`），玩家用方向键。
///
/// - `↑` → -Z（向屏幕远端走）
/// - `↓` → +Z（朝相机走）
/// - `←` → -X（左）
/// - `→` → +X（右）
///
/// `Without<Dead>` 是 unit 模块的全局约定（见该模块顶部文档）：死了的
/// unit 不走移动逻辑，躺到原地。
fn player_movement(
    keys: Res<ButtonInput<KeyCode>>,
    mut q_player: Query<&mut LinearVelocity, (With<Player>, Without<Dead>)>,
) {
    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowUp) {
        input.y -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowLeft) {
        input.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        input.x += 1.0;
    }
    let target_xz = if input.length_squared() > 0.0 {
        input.normalize() * PLAYER_SPEED
    } else {
        Vec2::ZERO
    };

    for mut vel in &mut q_player {
        // 只覆盖 X / Z；Y 留给重力，玩家会自然贴着地面。
        vel.x = target_xz.x;
        vel.z = target_xz.y;
    }
}

/// 按 `Space` 给玩家一次 10 点伤害 —— 走正规的 [`DamageMessage`] 通道，
/// 跟未来的敌人攻击共用结算路径，验证消息总线打通。
///
/// 只在 debug build 编译；release / dist 完全不存在这个 system。
#[cfg(debug_assertions)]
fn debug_damage_player_on_space(
    keys: Res<ButtonInput<KeyCode>>,
    mut damage: MessageWriter<DamageMessage>,
    q_player: Query<Entity, With<Player>>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    for entity in &q_player {
        damage.write(DamageMessage {
            target: entity,
            amount: 10.0,
        });
    }
}

/// 按 `R` 让玩家"满血复活"：清掉 [`Dead`] marker 并把 [`Health::current`]
/// 拉回 [`Health::max`]。注意没有 `Without<Dead>` filter —— 复活就是要
/// 对死了的人也生效。
///
/// 只在 debug build 编译。
#[cfg(debug_assertions)]
fn debug_revive_player_on_r(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut q_player: Query<(Entity, &mut Health), With<Player>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    for (entity, mut health) in &mut q_player {
        health.current = health.max;
        // remove::<Dead>() 对没挂 Dead 的 entity 也安全 —— Bevy 静默忽略。
        commands.entity(entity).remove::<Dead>();
    }
}
