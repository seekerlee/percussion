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
use bevy_asset_loader::prelude::*;

use super::movement::MoveVelocity;
use super::{Body, DamageMessage, Dead, Health, UNIT_BODY_HEIGHT, Unit};
use crate::app_state::AppState;
use crate::sprite_billboard::{BillboardSprite, PIXELS_PER_METER};

/// 玩家物理 body 半径（米）。
///
/// body 是 capsule，**总高**由共享常量 [`UNIT_BODY_HEIGHT`] 决定，这
/// 里只控制半径 = 顶视 XZ 上的推挤占位。必须 ≤ `UNIT_BODY_HEIGHT / 2`，
/// 否则 [`PLAYER_BODY_LENGTH`] 会负（capsule 无解）。选 capsule 而不是
/// sphere 是为了同享[`UNIT_BODY_HEIGHT`]的“并排接触法线纯水平”特性，
/// 不同 R 的 unit 互推时 Y 不会抖动。
///
/// 玩家落地后父 entity 位于 `y = UNIT_BODY_HEIGHT / 2`（capsule 中心
/// = 总高一半），与半径无关。
const PLAYER_BODY_RADIUS: f32 = 0.4;
/// capsule 的圆柱段长度（**不含**两端半球）—— avian `Collider::capsule`
/// 第二个参数要的就是这个。推导：总高 H = 2R + L → L = H - 2R。
const PLAYER_BODY_LENGTH: f32 = UNIT_BODY_HEIGHT - 2.0 * PLAYER_BODY_RADIUS;
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
/// 位于 `y_world = UNIT_BODY_HEIGHT / 2`（capsule 中心 = 总高一半）；
/// sprite mesh 中心应在 `y_world = sprite_height / 2`。所以子实体本地
/// Y = `(sprite_height - UNIT_BODY_HEIGHT) / 2`。
///
/// 当 sprite_height == UNIT_BODY_HEIGHT 时 offset = 0（当前 Player sprite
/// 刚好 2m，与 [`UNIT_BODY_HEIGHT`] 对齐 → 偏移为 0）。
const PLAYER_SPRITE_OFFSET_Y: f32 = (PLAYER_SPRITE_HEIGHT - UNIT_BODY_HEIGHT) * 0.5;
/// 玩家平移速度（米/秒）。
const PLAYER_SPEED: f32 = 5.0;
/// 玩家初始最大生命值。数值是 prototype 阶段的占位，等战斗公式立起来再调。
const PLAYER_MAX_HEALTH: f32 = 100.0;

/// 玩家用的预加载资产集合。
///
/// 由 [`bevy_asset_loader`] 在 [`AppState::Loading`] 阶段填充：宏自动
/// 生成的 `AssetCollection::create` 会 `asset_server.load_with_settings`
/// 出 handle、监控就绪、最后把这个结构体作为 `Resource` insert 进 World。
/// 因此在 [`AppState::InGame`] 的 `OnEnter` 或后续 system 里 `Res<PlayerAssets>`
/// 拿到时，`sprite` handle **保证已完成加载** —— 调用 `spawn_player` 不用再担
/// 心“贴图还在路上”。
///
/// # 采样器：nearest
///
/// `#[asset(image(sampler(filter = nearest)))]` 等同于手写 `ImageSamplerDescriptor::nearest()`
/// —— 保留像素边缘锐利，不让 linear 插值把像素艺术糊掉。
#[derive(AssetCollection, Resource)]
pub struct PlayerAssets {
    /// 玩家身体 sprite 贴图。当前是 128×64 单图（人物画在中间，左右大
    /// 片透明）。未来换 sprite sheet 时，可以在 `AssetCollection` 里加
    /// `texture_atlas_layout` 属性，让 bevy_asset_loader 直接吐 `Handle<TextureAtlasLayout>`。
    #[asset(path = "sprites/player.png")]
    #[asset(image(sampler(filter = nearest)))]
    pub sprite: Handle<Image>,
}

/// 玩家标记。
///
/// `#[require(...)]` 是 Bevy 0.15+ 的 required components 机制：spawn `Player`
/// 时 Bevy 自动挂上这些依赖组件 —— 语义上等于"`Player` 是一种 `Unit`，
/// 且无需手写的生命值初始为满血"。实现上是组合而非继承：组件都挂在
/// 同一 entity 上。
#[derive(Component, Debug, Default)]
#[require(Unit, Body, Health = Health::new(PLAYER_MAX_HEALTH))]
pub struct Player;

/// Player 插件 —— 注册键盘移动 system，以及 debug build 下的调试快捷键。
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // 把 PlayerAssets 挂到 AppState::Loading 阶段的 LoadingState 上。
        // 这一步要求 `AppStatePlugin` 已经 add 过 —— 见 lib.rs 里的注册顺序。
        // 等所有挂在此 LoadingState 上的 collection 都就绪，bevy_asset_loader
        // 会自动把 `PlayerAssets` insert 成 Resource，并把 state 切到 InGame。
        app.configure_loading_state(
            LoadingStateConfig::new(AppState::Loading).load_collection::<PlayerAssets>(),
        );

        // 玩家移动：每帧根据输入写 `LinearVelocity`，物理在 `FixedPostUpdate`
        // 64Hz 积分。配合 entity 上的 `TransformInterpolation`，资产同帧间插值到
        // 渲染帧率，避开「物理 tick 跳跳」造成的可见顿温。Update 里写位
        // FixedUpdate 里写都可以：`pressed` 是连续状态，多次写同一个值无损失，
        // Update 频率高一点起码保证下个物理 tick 看到的是最新输入。
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
/// - `player_assets`：[`PlayerAssets`] 资源，由 LoadingState 保证就绪
/// - `parent_stage`：[`spawn_stage`](crate::stage::spawn_stage) 返回的根 entity
/// - `local_pos`：stage 局部坐标系下的初始位置（Y > 0 让玩家从空中落下）
pub fn spawn_player(
    commands: &mut Commands,
    player_assets: &PlayerAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    parent_stage: Entity,
    local_pos: Vec3,
) -> Entity {
    // sprite 贴图：采样器（nearest）已经在 PlayerAssets 加载时设过，这里
    // 只需 clone handle 拿来当 material 的 base_color_texture。
    let sprite_mesh = meshes.add(Rectangle::new(PLAYER_SPRITE_WIDTH, PLAYER_SPRITE_HEIGHT));
    let sprite_material = materials.add(StandardMaterial {
        base_color_texture: Some(player_assets.sprite.clone()),
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
            // Kinematic 刚体：position / velocity / 重力 全部由游戏代码接管
            // （见 `unit/movement.rs` 顶部文档）。走动不是被 solver
            // 推出来的，是每帧 sweep-and-slide 主动推出来的 —— 互相挡却
            // 互不推动，适合 top-down ARPG 的 go-stop 手感。
            RigidBody::Kinematic,
            // capsule body：其他 unit 不同半径互推时接触点落在圆柱中段、
            // 法线纯水平，Y 方向不抽。总高 [`UNIT_BODY_HEIGHT`] 为全场
            // ground unit 共享，这里只需在调用点拼出 cylinder 段长度。
            Collider::capsule(PLAYER_BODY_RADIUS, PLAYER_BODY_LENGTH),
            // 防止被撞翻滚 —— 俯视斜角游戏角色应保持站立。不锁
            // 转动会被击飞 / 撞压之类的接触带动。Kinematic 下其实
            // solver 不会主动转动我们，但保留表达意图。
            LockedAxes::ROTATION_LOCKED,
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

/// 方向键移动玩家：每帧根据按键设置 X/Z 方向期望速度、写入 [`MoveVelocity`]，
/// Y 留给 [`apply_gravity`](crate::unit::movement)。
///
/// 为什么写 [`MoveVelocity`] 而不是直接改 Transform：输入是"期望走多远"，
/// 能不能走、能走多远由 sweep-and-slide 加上环境约束决定。玩家不应该直
/// 接改 Position，否则会穿模、穿墙、跳过另一个 unit。
///
/// 为什么不写 avian 的 `LinearVelocity`：见 [`MoveVelocity`] 文档 ——
/// 简言之，避免与 avian 位置集成器双重位移。
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
    mut q_player: Query<&mut MoveVelocity, (With<Player>, Without<Dead>)>,
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
        // 只覆盖 X / Z；Y 留给重力 / 击飞 impulse 之类的其他来源。
        vel.0.x = target_xz.x;
        vel.0.z = target_xz.y;
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
