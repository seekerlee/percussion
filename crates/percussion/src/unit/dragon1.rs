//! Dragon1 —— 第一只敌人（占位 sprite，验证通路用）。
//!
//! # 与 Unit / Player 的关系
//!
//! 跟 [`Player`](super::player::Player) 平级：[`Dragon1`] 也是一种
//! [`Unit`](super::Unit)，通过 `#[require(Unit)]` spawn 时自动补
//! `Unit` marker。这样通用 unit 机制（伤害结算、死亡转换）默认覆盖它，
//! Dragon1 专属 system 用 `With<Dragon1>` filter，跟 player / 其他 unit
//! 正交。
//!
//! # 当前只是占位
//!
//! 还没有 AI / 攻击逻辑，只是站在原地，验证 sprite 加载、billboard、
//! Unit 共享 system（伤害消息 / 死亡转换）通路。等出现第二种敌人、有
//! 共享行为（索敌、攻击窗口等）时再抽 `Enemy` 中间层 —— 现在抽属于
//! 猜测性扩展，不做。

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_asset_loader::prelude::*;
use bevy_sprite3d::prelude::*;

pub mod animation;

use animation::{Dragon1AnimationPlugin, Dragon1AnimationState};
use super::hurtbox::spawn_hurtbox;
use super::{Body, Health, UNIT_BODY_HEIGHT, Unit};
use crate::app_state::AppState;
use crate::physics_layers::GameLayer;
use crate::sprite_billboard::{BillboardSprite, PIXELS_PER_METER};

// ============================================================================
// TODO（你来填）：以下常量是占位值，按 sprite 实际像素 / 期望血量改。
// ============================================================================

/// Dragon1 物理 body 半径（米）。body 是 capsule，**总高**由共享常量
/// [`UNIT_BODY_HEIGHT`] 决定，这里只控制半径 = 俯视 XZ 上的推挤占位。
///
/// 与玩家同取 0.4。同高的 capsule 互推时 —— 以后加出不同 R 的 unit
/// 也不会 Y 抽（原理见 [`UNIT_BODY_HEIGHT`] 文档）。必须 ≤
/// `UNIT_BODY_HEIGHT / 2`，否则 capsule length 负。
const DRAGON1_BODY_RADIUS: f32 = 0.4;
/// capsule 的圆柱段长度（不含两端半球）—— H = 2R + L → L = H - 2R。
const DRAGON1_BODY_LENGTH: f32 = UNIT_BODY_HEIGHT - 2.0 * DRAGON1_BODY_RADIUS;

/// Dragon1 初始最大生命值。
const DRAGON1_MAX_HEALTH: f32 = 50.0;

// ============================================================================
// 上面是要填的；下面是由它们推导出来的，一般不用改。
// ============================================================================

/// sprite 子实体相对父实体的 Y 偏移（米）。推导同
/// [`PLAYER_SPRITE_OFFSET_Y`](super::player) —— 配合
/// [`Sprite3d::pivot`] = `(0.5, 0.0)` 让贴图“脚中”对齐 sprite mesh
/// 局部 (0,0)，所以子实体局部 Y 直接抵消父 entity（capsule 中心）
/// 到地面的距离。
const DRAGON1_SPRITE_OFFSET_Y: f32 = -UNIT_BODY_HEIGHT * 0.5;

/// Dragon1 的预加载资产集合。
///
/// 行为完全等价于 [`PlayerAssets`](super::player::PlayerAssets) ——
/// [`bevy_asset_loader`] 在 [`AppState::Loading`] 阶段把 sprite 加载好、
/// 整个 collection 作为 `Resource` insert，进入 [`AppState::InGame`] 后
/// `Res<Dragon1Assets>` 拿到的 handle 保证就绪。
///
/// nearest sampler 保留像素边缘锐利（同 player）。
#[derive(AssetCollection, Resource)]
pub struct Dragon1Assets {
    /// Dragon1 sprite sheet —— 1 行 9 帧的扇翅膀循环（192×176 per frame）。
    ///
    /// **文件路径必须存在**：`crates/percussion/assets/sprites/units/dragon1/sunny-dragon-fly.png`。
    /// 缺文件 `bevy_asset_loader` 会让 LoadingState 永不就绪，游戏卡在
    /// Loading 黑屏 —— 这是符合预期的“硬依赖”，不要静默 fallback。
    #[asset(path = "sprites/units/dragon1/sunny-dragon-fly.png")]
    #[asset(image(sampler(filter = nearest)))]
    pub sheet: Handle<Image>,
    /// 把 sheet 切成 frame index 0..9 的 atlas 布局 —— 跟
    /// [`PlayerAssets::layout`](super::player::PlayerAssets) 同套机制。
    /// `tile_size_x/y` 与列数 / 行数必须跟实际 PNG 匹配，否则采到错误
    /// 区域。
    #[asset(texture_atlas_layout(tile_size_x = 192, tile_size_y = 176, columns = 9, rows = 1))]
    pub layout: Handle<TextureAtlasLayout>,
}

/// Dragon1 标记。
///
/// `#[require(...)]`：spawn `Dragon1` 时 Bevy 自动挂上 `Unit` marker、
/// 满血 `Health`、动画状态 —— 跟 [`Player`](super::player::Player) 用同
/// 一套机制。
#[derive(Component, Debug, Default)]
#[require(Unit, Body, Health = Health::new(DRAGON1_MAX_HEALTH), Dragon1AnimationState)]
pub struct Dragon1;

/// Dragon1 插件 —— 注册 `AssetCollection`，触发 sprite 在 Loading state
/// 加载。注意 **必须在 [`AppStatePlugin`](crate::app_state::AppStatePlugin)
/// 之后** add，否则 `LoadingState` 还没注册会 panic。
pub struct Dragon1Plugin;

impl Plugin for Dragon1Plugin {
    fn build(&self, app: &mut App) {
        app.configure_loading_state(
            LoadingStateConfig::new(AppState::Loading).load_collection::<Dragon1Assets>(),
        );
        app.add_plugins(Dragon1AnimationPlugin);
    }
}

/// 在指定 stage 下 spawn 一只 dragon1，返回 entity。
///
/// 结构跟 [`spawn_player`](super::player::spawn_player) 同构：父 entity
/// 只挂物理 / 逻辑，sprite 是子实体（带 [`BillboardSprite`]），LocalTransform
/// 抬高让"脚"贴地面。
///
/// # 参数
///
/// - `assets`：[`Dragon1Assets`] 资源，由 LoadingState 保证就绪
/// - `parent_stage`：[`spawn_stage`](crate::stage::spawn_stage) 返回的根 entity
/// - `local_pos`：stage 局部坐标系下的初始位置（Y > 0 让它从空中落下）
pub fn spawn_dragon1(
    commands: &mut Commands,
    assets: &Dragon1Assets,
    parent_stage: Entity,
    local_pos: Vec3,
) -> Entity {
    let entity = commands
        .spawn((
            Dragon1,
            Transform::from_translation(local_pos),
            // 同 [`spawn_player`](super::player::spawn_player)：父 entity 不渲染，
            // 但 sprite 子 entity 带 `Visibility`，需要父级也有以构成完整
            // 继承链，否则报 B0004。
            Visibility::default(),
            // Kinematic 刚体：跟 player 同模式（见 `unit/movement.rs` 顶部
            // 文档）。dragon1 还没 AI，`MoveVelocity` 默认 (0,0,0)，唯一
            // 不为零的来源是重力 —— spawn 在空中 → 自然落地 → on_ground
            // 后重力归零、原地站立。等 AI 接入只是"多一个写
            // MoveVelocity.xz 的来源"，movement 通路本身不变。
            RigidBody::Kinematic,
            // capsule body，选型思路同 [`Player`](super::player::Player)。总高
            // [`UNIT_BODY_HEIGHT`] 全场 ground unit 共享，不同 R 互推时接触
            // 法线纯水平，Y 不抽。
            Collider::capsule(DRAGON1_BODY_RADIUS, DRAGON1_BODY_LENGTH),
            // CollisionLayers 同 player：body 只跟 body / terrain 互推，不
            // 被 hurtbox / hitbox sensor 干扰。分层详见 [`crate::physics_layers`]。
            CollisionLayers::new(GameLayer::Body, [GameLayer::Body, GameLayer::Terrain]),
            LockedAxes::ROTATION_LOCKED,
            ChildOf(parent_stage),
        ))
        .id();

    // sprite 子实体 —— 见 [`spawn_player`](super::player::spawn_player) 的
    // 同名块文档，结构与说明一致。atlas 用 sheet + layout 组合，初始
    // index = 0；每帧由 [`animation::tick_dragon1_animation`] 推进。
    commands.spawn((
        BillboardSprite,
        Sprite3d {
            pixels_per_metre: PIXELS_PER_METER,
            unlit: true,
            pivot: Some(Vec2::new(0.5, 0.0)),
            ..default()
        },
        Sprite::from_atlas_image(
            assets.sheet.clone(),
            TextureAtlas {
                layout: assets.layout.clone(),
                index: 0,
            },
        ),
        Transform::from_translation(Vec3::new(0.0, DRAGON1_SPRITE_OFFSET_Y, 0.0)),
        ChildOf(entity),
    ));

    // 受击判定：同 [`spawn_player`](super::player::spawn_player)，当前用跟
    // body 同型的 capsule 覆盖整个角色。
    spawn_hurtbox(
        commands,
        entity,
        Collider::capsule(DRAGON1_BODY_RADIUS, DRAGON1_BODY_LENGTH),
        Transform::IDENTITY,
    );

    entity
}
