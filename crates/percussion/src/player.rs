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
use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;

use crate::sprite_billboard::{BillboardSprite, PIXELS_PER_METER};
use crate::unit::{DamageMessage, Dead, Health, Unit};

/// 玩家物理盒边长（米）—— 跟视觉 sprite 尺寸独立。玩家踋在地面
/// 上时，盒中心在 y=0.5（全尺寸 1m 立方体，半高 0.5）。
const PLAYER_COLLIDER_SIZE: f32 = 1.0;
/// 玩家 sprite 贴片尺寸（像素）。
///
/// 当前贴图是 128×64 单图：人物画在中间，左右两侧大片透明。透明像
/// 素不渲染，mesh 按贴图原始比例建立即可。换图改这两个常数。
///
/// 128 px ÷ [`PIXELS_PER_METER`] = 4 m 宽；64 px = 2 m 高。视觉上角色只占
/// 中间一小块，不影响显示正确性。
const PLAYER_SPRITE_PIXELS_WIDTH: f32 = 128.0;
const PLAYER_SPRITE_PIXELS_HEIGHT: f32 = 64.0;
const PLAYER_SPRITE_WIDTH: f32 = PLAYER_SPRITE_PIXELS_WIDTH / PIXELS_PER_METER;
const PLAYER_SPRITE_HEIGHT: f32 = PLAYER_SPRITE_PIXELS_HEIGHT / PIXELS_PER_METER;
/// sprite 子实体相对父实体的 Y 偏移（米）。
///
/// 推导：让 sprite 的**脚**贴地面（y_world = 0）。玩家落地后父 entity
/// 位于 y_world = collider_size / 2；sprite mesh 中心应在 y_world =
/// sprite_height / 2。所以偏移 = sprite_height/2 - collider_size/2。
const PLAYER_SPRITE_OFFSET_Y: f32 = (PLAYER_SPRITE_HEIGHT - PLAYER_COLLIDER_SIZE) * 0.5;
/// 玩家平移速度（米/秒）。
const PLAYER_SPEED: f32 = 5.0;
/// 玩家初始最大生命值。数值是 prototype 阶段的占位，等战斗公式立起来再调。
const PLAYER_MAX_HEALTH: f32 = 100.0;
/// 玩家 sprite 贴图资产路径（相对 `assets/`）。
const PLAYER_SPRITE_ASSET: &str = "sprites/player.png";

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
        // 放 `Update`：直接按 `delta_time` 加 Transform 位移，不走 avian 的
        // 速度积分。这样输入→位移→渲染全在同一帧、变帧率响应，避开
        // 物理 FixedPostUpdate 64Hz 节拍带来的输入延迟。avian 的 sync 默认
        // 双向（见 `PhysicsTransformConfig`），下一个物理 tick 会把我们
        // 写的 Transform 同步到内部 Position，碰撞 / 重力照常工作。
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
/// # 视觉 vs 物理拆开
///
/// player entity 本身只挂物理 / 逻辑（Collider / Health / RigidBody），
/// 不挂 mesh。视觉部分是一个子实体：带 [`BillboardSprite`] 的 2D 贴片，
/// LocalTransform 抬高使“脚”贴地面。这样 sprite 尺寸跟物理盒尺寸互
/// 不干扰，未来加影子 sprite / 武器 sprite 也就是多加几个子实体的事。
///
/// # 参数
///
/// - `parent_stage`：[`spawn_stage`](crate::stage::spawn_stage) 返回的根 entity
/// - `local_pos`：stage 局部坐标系下的初始位置（Y > 0 让玩家从空中落下）
pub fn spawn_player(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    parent_stage: Entity,
    local_pos: Vec3,
) -> Entity {
    // sprite 贴图：nearest filter 保留像素边缘锐利，不要 linear 插值变糊。
    let texture: Handle<Image> = asset_server.load_with_settings(
        PLAYER_SPRITE_ASSET,
        |settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
        },
    );
    let sprite_mesh = meshes.add(Rectangle::new(PLAYER_SPRITE_WIDTH, PLAYER_SPRITE_HEIGHT));
    let sprite_material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        // Mask：alpha > cutoff 不透，否则完全透 —— 贴图边缘干净。不用 Blend
        // 是因为 Blend 要按深度排序，多个 sprite 重叠时会闪。
        alpha_mode: AlphaMode::Mask(0.5),
        // unlit：不让 3D 灯光"加工"手绘贴图颜色，保留原貌 —— 饮荒风格的关键。
        unlit: true,
        // 双面渲染：billboard 转动过程中背面也可能被看到，不能被 cull。
        cull_mode: None,
        ..default()
    });

    let player_entity = commands
        .spawn((
            Player,
            Transform::from_translation(local_pos),
            // Dynamic 刚体：靠重力落到地面，靠 stage 物理屏障挡 XZ 移动。
            RigidBody::Dynamic,
            Collider::cuboid(
                PLAYER_COLLIDER_SIZE,
                PLAYER_COLLIDER_SIZE,
                PLAYER_COLLIDER_SIZE,
            ),
            // 防止被撞翻滚 —— 俯视斜角游戏角色应保持站立。
            LockedAxes::ROTATION_LOCKED,
            // 禁用 sleeping：avian 默认会把静止的 Dynamic body 标记为 Sleeping
            // 跳过积分省 CPU，但代价是被重新唤醒时位移会延迟 1-2 物理 tick，
            // 表现为"按方向键先顿一下才动"。player 这种随时被输入驱动的实
            // 体本来就不应该睡，多一次空积分相比手感损失完全划算。
            SleepingDisabled,
            // Bevy 0.18 relationship API：把自己挂成 parent_stage 的子实体。
            ChildOf(parent_stage),
        ))
        .id();

    // sprite 子实体：独立的视觉结点。LocalTransform 抬高是为了让贴片的
    // "脚"落在地面上，而不是穿出物理盒中央。
    commands.spawn((
        BillboardSprite,
        Mesh3d(sprite_mesh),
        MeshMaterial3d(sprite_material),
        Transform::from_translation(Vec3::new(0.0, PLAYER_SPRITE_OFFSET_Y, 0.0)),
        ChildOf(player_entity),
    ));

    player_entity
}

/// 方向键移动玩家：直接按 `delta_time` 在 Transform 上加 X/Z 位移；Y 留给
/// 物理（重力 + 地面碰撞）。这避开 avian 物理 tick 节拍带来的输入延迟，
/// 输入 → 位移 → 渲染同帧完成，按键即响应。
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
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut q_player: Query<&mut Transform, (With<Player>, Without<Dead>)>,
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
    if input.length_squared() == 0.0 {
        return;
    }
    let dir = input.normalize();
    let dt = time.delta_secs();
    for mut transform in &mut q_player {
        // 只动 X / Z；Y 不碰，让重力 / 地面碰撞继续管。
        transform.translation.x += dir.x * PLAYER_SPEED * dt;
        transform.translation.z += dir.y * PLAYER_SPEED * dt;
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
