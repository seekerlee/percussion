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

use super::{Body, Health, UNIT_BODY_HEIGHT, Unit};
use crate::app_state::AppState;
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

/// sprite 贴片的像素尺寸。
///
/// 改这两个常数 = 改贴片在世界里的米数（÷ [`PIXELS_PER_METER`]）。
/// 项目约定 32 px = 1 m，详见 `doc/units-and-assets.md`。
const DRAGON1_SPRITE_PIXELS_WIDTH: f32 = 64.0;
const DRAGON1_SPRITE_PIXELS_HEIGHT: f32 = 64.0;

/// Dragon1 初始最大生命值。
const DRAGON1_MAX_HEALTH: f32 = 50.0;

// ============================================================================
// 上面是要填的；下面是由它们推导出来的，一般不用改。
// ============================================================================

const DRAGON1_SPRITE_WIDTH: f32 = DRAGON1_SPRITE_PIXELS_WIDTH / PIXELS_PER_METER;
const DRAGON1_SPRITE_HEIGHT: f32 = DRAGON1_SPRITE_PIXELS_HEIGHT / PIXELS_PER_METER;
/// sprite 子实体相对父实体的 Y 偏移（米）—— 让 sprite 的"脚"贴地面。
///
/// 推导同 [`player`](super::player) 的 `PLAYER_SPRITE_OFFSET_Y`：父 entity
/// 落地后中心在 `y = UNIT_BODY_HEIGHT / 2`（capsule 中心 = 总高一半），
/// sprite mesh 中心应在 `y = sprite_height / 2`，所以偏移 =
/// `(sprite_height - UNIT_BODY_HEIGHT) / 2`。
///
/// 当前 Dragon1 sprite 刚好 2m，与 [`UNIT_BODY_HEIGHT`] 对齐 → 偏移为 0。
const DRAGON1_SPRITE_OFFSET_Y: f32 = (DRAGON1_SPRITE_HEIGHT - UNIT_BODY_HEIGHT) * 0.5;

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
    /// Dragon1 身体 sprite 贴图。
    ///
    /// **文件路径必须存在**：`crates/percussion/assets/sprites/dragon1.png`。
    /// 缺文件 `bevy_asset_loader` 会让 LoadingState 永不就绪，游戏卡在
    /// Loading 黑屏 —— 这是符合预期的"硬依赖"，不要静默 fallback。
    #[asset(path = "sprites/dragon1.png")]
    #[asset(image(sampler(filter = nearest)))]
    pub sprite: Handle<Image>,
}

/// Dragon1 标记。
///
/// `#[require(Unit, Health = ...)]`：spawn `Dragon1` 时 Bevy 自动挂上
/// `Unit` marker 和满血 `Health`，跟 [`Player`](super::player::Player)
/// 用同一套机制。
#[derive(Component, Debug, Default)]
#[require(Unit, Body, Health = Health::new(DRAGON1_MAX_HEALTH))]
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
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    parent_stage: Entity,
    local_pos: Vec3,
) -> Entity {
    let sprite_mesh = meshes.add(Rectangle::new(DRAGON1_SPRITE_WIDTH, DRAGON1_SPRITE_HEIGHT));
    let sprite_material = materials.add(StandardMaterial {
        base_color_texture: Some(assets.sprite.clone()),
        // Mask：alpha > cutoff 不透，否则完全透 —— 抠图边缘干脆。
        alpha_mode: AlphaMode::Mask(0.5),
        // unlit：保留贴图原貌，不让 3D 光照"加工"手绘色。
        unlit: true,
        // 双面渲染：billboard 转动过程中背面也可能被看到。
        cull_mode: None,
        ..default()
    });

    let entity = commands
        .spawn((
            Dragon1,
            Transform::from_translation(local_pos),
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
            LockedAxes::ROTATION_LOCKED,
            ChildOf(parent_stage),
        ))
        .id();

    commands.spawn((
        BillboardSprite,
        Mesh3d(sprite_mesh),
        MeshMaterial3d(sprite_material),
        Transform::from_translation(Vec3::new(0.0, DRAGON1_SPRITE_OFFSET_Y, 0.0)),
        ChildOf(entity),
    ));

    entity
}
