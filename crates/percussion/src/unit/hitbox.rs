//! Hitbox —— unit 的"攻击判定盒"。
//!
//! # 这个模块解决什么问题
//!
//! [`Hurtbox`](super::Hurtbox) 回答的是"我能被打中的范围"。本模块的 [`Hitbox`]
//! 回答的是反过来那一半：**"我这一刀 / 这颗子弹的伤害判定范围在哪里"**。
//!
//! 一次攻击的生命周期：
//!
//! 1. 攻击源（技能 active 阶段、投射物出膛、陷阱触发）调用 [`spawn_hitbox`]
//!    在世界里放一块 sensor；
//! 2. 物理层让这块 sensor 跟所有 [`Hurtbox`](super::Hurtbox) 测重叠
//!    （[`crate::physics_layers`] 里 `PlayerHitbox` / `EnemyHitbox` 都只 filter
//!    `Hurtbox`，反之亦然）；
//! 3. [`detect_hitbox_collisions`] 把"hitbox 撞 hurtbox"翻译成
//!    [`DamageMessage`](super::DamageMessage)，伤害结算交给
//!    [`apply_damage_messages`](super::apply_damage_messages)；
//! 4. [`tick_hitbox_lifetime`] 计时到点自动 despawn —— hitbox 是一次性的，活几帧。
//!
//! # 为什么 hitbox 是独立 entity 而不是挂在攻击者身上
//!
//! - **形状跟攻击者本体解耦**：一刀挥出去的盒子在攻击者前方 0.7m、宽 1.2m，
//!   跟 [`Body`](super::Body) capsule 完全不同；挂同一个 entity 上要切 collider
//!   非常脏。
//! - **多块 hitbox 共存**：一招可能甩 3 个判定（连段、范围 + 中心），独立 entity
//!   每块各自管自己的 lifetime / hits 列表。
//! - **跟攻击者 transform 解耦**：投射物出膛后跟着自己走，不应该跟攻击者位移；
//!   即便是近战 swing，做成"出招瞬间快照位置 + 短 lifetime"比"实时跟随"更易调。
//!
//! 因此 [`spawn_hitbox`] **不**把 hitbox 挂为 owner 的 `ChildOf` 子实体 ——
//! 跟 [`spawn_hurtbox`](super::spawn_hurtbox) 在这点上是反的（hurtbox 要跟着
//! unit 走、要跟着 despawn；hitbox 是 fire-and-forget）。
//!
//! # 友军误伤
//!
//! 当前过滤规则极简：[`detect_hitbox_collisions`] 只跳过
//! `hurtbox.owner == hitbox.owner`（自己不打自己）。也就是说：
//!
//! - 玩家方 vs 敌方 —— 互相能打（物理层就这么设的）；
//! - 多个玩家 / 多个敌方之间 —— 当前**会**互相误伤。
//!
//! 项目当下只有一个玩家 + 龙，这条规则够用。将来加同侧多 unit 时，要么给
//! [`Hurtbox`](super::Hurtbox) 也加 `Faction` 字段做 hitbox×hurtbox 过滤，要么
//! 用 avian 的 `CollisionLayers` 进一步细分层 —— 出现需求时再加，不预先抽象。
//!
//! # 跟技能 / 投射物的关系
//!
//! 本模块只提供"hitbox 这块判定 + 命中→DamageMessage"的最小通路，
//! **不**关心谁创建了 hitbox。技能子系统 [`super::skill`] 在
//! [`SkillActivatedMessage`](super::skill::SkillActivatedMessage) 时调
//! [`spawn_hitbox`]；将来投射物 / 陷阱 / 环境伤害也都是直接调
//! [`spawn_hitbox`]。这种"上游分散调用 + 下游统一结算"跟 hurtbox 的设计同构。

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::physics_layers::GameLayer;

use super::DamageMessage;
use super::hurtbox::Hurtbox;

/// Unit 的归属阵营 —— 当前用来在 [`spawn_hitbox`] 决定 hitbox 走
/// [`PlayerHitbox`](GameLayer::PlayerHitbox) 还是
/// [`EnemyHitbox`](GameLayer::EnemyHitbox) 层。
///
/// 这个类型**逻辑上属于 unit 通用概念**（unit 本身有阵营），但因为目前只有
/// hitbox 子系统读它，先放这里。等 hurtbox / AI / UI 也要读阵营时再上移到
/// [`super`]（unit 顶层）。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Player,
    Enemy,
}

/// 标记一个 entity 是某次攻击的判定盒。
///
/// 跟 [`Hurtbox`](super::Hurtbox) 是对偶身份 marker：hurtbox 回答"谁挨打"，
/// hitbox 回答"谁打人 + 打多少"。物理形状 / sensor / 分层走 avian 自己的
/// 组件，跟本 marker 解耦。
#[derive(Component, Debug)]
pub struct Hitbox {
    /// 攻击发起者 entity —— 用来：
    /// - 自损过滤（[`detect_hitbox_collisions`] 跳过 `hurtbox.owner == hitbox.owner`）；
    /// - 后续做"是谁打死的我"统计 / 击杀归属。
    pub owner: Entity,
    /// 一次命中造成的伤害（最终值，结算时不再修改 —— 增减伤逻辑应该已经在
    /// 创建 hitbox 之前折算好，或者将来在 `apply_damage_messages` 里再统一改）。
    pub damage: f32,
}

/// Hitbox 的剩余寿命（秒）。归零时由 [`tick_hitbox_lifetime`] 自动 despawn。
///
/// 拆成独立组件而不是塞进 [`Hitbox`] 是因为：lifetime 是"我什么时候消失"，
/// 是寿命管理；damage / owner 是"我是谁的攻击"，是身份。两者的修改时机和
/// 关心方都不一样，分开放更清晰。
#[derive(Component, Debug)]
pub struct HitboxLifetime {
    pub remaining: f32,
}

/// 这块 hitbox 已经命中过的 unit 列表 —— 防止一块持续几帧的判定盒对同一个目标
/// 反复发 [`DamageMessage`](super::DamageMessage)。
///
/// 一次挥砍可能横跨 3–5 帧，物理 sensor 在这段时间内每帧都看到目标在重叠区里；
/// 如果不去重，一刀变 5 倍伤害。
#[derive(Component, Debug, Default)]
pub struct HitboxHits {
    /// 这里记的是被命中 unit 的 entity（即 `hurtbox.owner`），不是 hurtbox 自己 ——
    /// 一个 unit 可能挂多块 hurtbox（头 / 身 / 腿），但应该算作"同一个人"只打一次。
    pub already_hit: Vec<Entity>,
}

/// 在世界里 spawn 一块 hitbox，返回它的 entity。
///
/// 调用方负责传：
///
/// - `owner`：攻击发起者；
/// - `faction`：决定走哪一层（[`GameLayer::PlayerHitbox`] / [`GameLayer::EnemyHitbox`]）；
/// - `collider`：形状（盒 / 球 / 胶囊 / 圆锥扇形 …）；
/// - `transform`：**世界坐标**位姿（注意：hitbox 不 `ChildOf` owner，所以
///   调用方需要自己把 owner 的当前 transform 折算进去 —— 一般是
///   `owner_transform * Transform::from_translation(forward * offset)`）；
/// - `damage`：一次命中的伤害；
/// - `lifetime`：存活秒数。一般跟技能 active 阶段一样长（短 swing）或更长（投射物）。
///
/// # 自动挂上的组件
///
/// - [`Hitbox { owner, damage }`](Hitbox)；
/// - [`HitboxLifetime { remaining: lifetime }`](HitboxLifetime)；
/// - [`HitboxHits::default()`](HitboxHits)；
/// - 传入的 `collider`；
/// - [`Sensor`] —— 跟 hurtbox 一样，只感应不解算，避免把目标推开 / 被
///   [`MoveAndSlide`](avian3d::prelude::MoveAndSlide) 当障碍；
/// - [`CollisionLayers`] —— membership = `PlayerHitbox` / `EnemyHitbox`（看 faction），
///   filter = `[Hurtbox]`；
/// - [`CollidingEntities::default()`](CollidingEntities) —— 让 avian 把"当前跟我重叠
///   的 entity 集合"写到这里，[`detect_hitbox_collisions`] 直接读；
/// - 传入的 `transform`（世界坐标）。
pub fn spawn_hitbox(
    commands: &mut Commands,
    owner: Entity,
    faction: Faction,
    collider: Collider,
    transform: Transform,
    damage: f32,
    lifetime: f32,
) -> Entity {
    let membership = match faction {
        Faction::Player => GameLayer::PlayerHitbox,
        Faction::Enemy => GameLayer::EnemyHitbox,
    };
    commands
        .spawn((
            Hitbox { owner, damage },
            HitboxLifetime {
                remaining: lifetime,
            },
            HitboxHits::default(),
            collider,
            Sensor,
            CollisionLayers::new(membership, [GameLayer::Hurtbox]),
            CollidingEntities::default(),
            transform,
        ))
        .id()
}

/// 每帧扣 hitbox 寿命，归零的 despawn。
///
/// 用绝对秒数倒计而不是"帧数"是为了跟物理 / 动画对齐 —— hitbox 是按真实时间
/// "active 0.05s"这种语义存在的，跟帧率无关。
fn tick_hitbox_lifetime(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut HitboxLifetime)>,
) {
    let dt = time.delta_secs();
    for (entity, mut lifetime) in &mut q {
        lifetime.remaining -= dt;
        if lifetime.remaining <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// 每帧扫所有 hitbox 的 [`CollidingEntities`]，对新命中的 hurtbox 发
/// [`DamageMessage`](super::DamageMessage)。
///
/// 过滤规则（按出现概率排序）：
/// 1. 对方不是 hurtbox —— 跳过（理论上不会发生，因为物理层 filter 只让 hitbox
///    撞 hurtbox，但保险起见 query miss 直接跳）；
/// 2. `hurtbox.owner == hitbox.owner` —— 自己不打自己；
/// 3. `hitbox.already_hit` 已经记过这个 owner —— 不重复发伤害。
///
/// 通过三道之后，把 owner 加进 `already_hit` 并 write 一条 DamageMessage。
fn detect_hitbox_collisions(
    mut q_hitbox: Query<(&Hitbox, &CollidingEntities, &mut HitboxHits)>,
    q_hurtbox: Query<&Hurtbox>,
    mut damages: MessageWriter<DamageMessage>,
) {
    for (hitbox, colliding, mut hits) in &mut q_hitbox {
        for &other in colliding.iter() {
            let Ok(hurtbox) = q_hurtbox.get(other) else {
                continue;
            };
            if hurtbox.owner == hitbox.owner {
                continue;
            }
            if hits.already_hit.contains(&hurtbox.owner) {
                continue;
            }
            hits.already_hit.push(hurtbox.owner);
            damages.write(DamageMessage {
                target: hurtbox.owner,
                amount: hitbox.damage,
            });
        }
    }
}

/// HitboxPlugin —— hitbox 子系统的注册点。
///
/// 跟 [`HurtboxPlugin`](super::HurtboxPlugin) 配对，但这边有实际 system：
/// 寿命倒计 + 命中检测。两个 system 互相独立，没 chain。
pub struct HitboxPlugin;

impl Plugin for HitboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tick_hitbox_lifetime, detect_hitbox_collisions));
    }
}
