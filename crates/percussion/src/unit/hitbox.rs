//! Hitbox —— unit 的"攻击判定盒"+ 命中规格的载体。
//!
//! # 这个模块解决什么问题
//!
//! [`Hurtbox`](super::Hurtbox) 回答的是"我能被打中的范围"。本模块的 [`Hitbox`]
//! 回答的是反过来那一半：**"我这一刀 / 这颗子弹的伤害判定范围在哪里 +
//! 打中之后该发生什么"**。
//!
//! 一次攻击的生命周期：
//!
//! 1. 攻击源（技能 active 阶段、投射物出膛、陷阱触发）调用 [`spawn_hitbox`]
//!    在世界里放一块 sensor，附带一份 [`HitSpec`]（命中规格）；
//! 2. 物理层让这块 sensor 跟所有 [`Hurtbox`](super::Hurtbox) 测重叠
//!    （[`crate::physics_layers`] 里 `PlayerHitbox` / `EnemyHitbox` 都只 filter
//!    `Hurtbox`，反之亦然）；
//! 3. [`detect_hitbox_collisions`] 把"hitbox 撞 hurtbox"翻译成
//!    [`CollisionMessage`]（最原始的"谁打中了谁"，结算还没发生）；
//! 4. [`damage_calc`](super::damage_calc) 跑 [`HitSpec::modifiers`] 流水线，写血，
//!    发 [`DamageDealtMessage`](super::DamageDealtMessage)；
//! 5. [`hit_triggers`](super::hit_triggers) 按 [`HitSpec::triggers`] 派发吸血 /
//!    点燃 / 击退等副作用；
//! 6. [`tick_hitbox_lifetime`] 计时到点自动 despawn —— hitbox 是一次性的，活几帧。
//!
//! # 两阶段拆分：modifier vs trigger
//!
//! [`HitSpec`] 把命中后果拆成两条 Vec：
//!
//! - [`modifiers`](HitSpec::modifiers) —— **影响伤害数字**，顺序敏感串行执行
//!   （`Mul(2.0)` 在 `Crit{chance:0.5, mul:2.0}` 之前 vs 之后，最终值不同）。
//! - [`triggers`](HitSpec::triggers) —— **不影响伤害数字、只挂副作用**，相互独立
//!   （吸血、击退、点燃；顺序原则上无所谓）。
//!
//! 拆开是因为流水线必须先跑完 modifier 才知道最终伤害 + 是否暴击，副作用
//! 里像 [`HitTrigger::CritOnly`] / [`HitTrigger::Lifesteal`] 要读这两个结果。
//!
//! # caster-side 一切烧在 spawn 那一刻
//!
//! 这是个**重要约定**：bridge（[`super::skill_hitbox`] / 投射物 / 陷阱）在调
//! [`spawn_hitbox`] 之前，要把"caster 的 Strength / 武器倍率 / 全局 buff"
//! 全部读出来、折算成具体的 [`DamageModifier::Mul`] 值塞进 [`HitSpec::modifiers`]。
//!
//! 这样命中那一刻不需要再回查 caster（caster 可能已死 / 已 despawn / 状态变了
//! 也不影响），世界状态简洁。代价是 caster 状态变化后已飞出去的攻击不感知 ——
//! 但这正是想要的行为（射出去的箭不会突然变更狠）。
//!
//! # 为什么 hitbox 是独立 entity 而不是挂在攻击者身上
//!
//! - **形状跟攻击者本体解耦**：一刀挥出去的盒子在攻击者前方 0.7m、宽 1.2m，
//!   跟 [`Body`](super::Body) capsule 完全不同。
//! - **多块 hitbox 共存**：一招可能甩 3 个判定（连段、范围 + 中心），独立 entity
//!   各自管自己的 lifetime / hits 列表。
//! - **跟攻击者 transform 解耦**：投射物出膛后跟着自己走；近战 swing"出招瞬间
//!   快照位置 + 短 lifetime"比"实时跟随"更易调。
//!
//! 因此 [`spawn_hitbox`] **不**把 hitbox 挂为 owner 的 `ChildOf` 子实体。
//!
//! # 友军误伤
//!
//! [`detect_hitbox_collisions`] 只跳过 `hurtbox.owner == hitbox.owner`
//! （自己不打自己）。当前项目只有一个玩家 + 龙，够用。同侧多 unit 互相
//! 误伤问题等出现需求时再加 Faction 过滤。

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::physics_layers::GameLayer;

use super::DamagePipeline;
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

/// "这次命中"的完整声明式规格 —— 由调用 [`spawn_hitbox`] 的一方填好之后
/// 烧进 hitbox entity，之后**不再修改**。
///
/// 见模块文档 "两阶段拆分" + "caster-side 一切烧在 spawn 那一刻"。
#[derive(Debug, Clone)]
pub struct HitSpec {
    /// modifier pipeline 的输入。所有 [`Mul`](DamageModifier::Mul) /
    /// [`Crit`](DamageModifier::Crit) 都从这个值起跑。
    pub base_damage: f32,
    /// 修正流水线 —— 顺序敏感，[`damage_calc`](super::damage_calc) 顺序串行执行。
    pub modifiers: Vec<DamageModifier>,
    /// 命中后挂副作用 —— [`hit_triggers`](super::hit_triggers) 顺序遍历但
    /// 互不依赖（顺序不影响结果）。
    pub triggers: Vec<HitTrigger>,
}

/// 影响最终伤害**数值**的修正。
///
/// 多个 modifier 按 Vec 顺序串行：`amount` 从 `base_damage` 起跑，
/// 每个 modifier 在前一步结果上继续 apply。
#[derive(Debug, Clone, Copy)]
pub enum DamageModifier {
    /// 乘法系数。bridge 把 caster.Strength / 武器倍率 / 全局 buff 加成
    /// 都烧成具体 Mul 值塞进来。多个 Mul 顺序连乘。
    Mul(f32),
    /// 暴击 roll —— `chance` 概率成功；成功则当前 `amount` ×= `mul`、
    /// 流水线输出 `is_crit = true`。多个 Crit 各自独立 roll，任一成功
    /// 即标记 is_crit；倍率连乘。
    Crit { chance: f32, mul: f32 },
}

/// 命中之后挂的副作用。在 damage 已写入目标 Health 之后由
/// [`hit_triggers`](super::hit_triggers) 派发。
#[derive(Debug, Clone)]
pub enum HitTrigger {
    /// 把 `final_amount * ratio` 的血量回给 caster（即 [`Hitbox::owner`]）。
    Lifesteal { ratio: f32 },
    /// 沿 caster→target 方向以 `force` 强度推 target。
    ///
    /// 首版未实现 —— 占位符，等 movement / impulse 子系统接入。
    Knockback { force: f32 },
    /// 在 target 上挂一个 [`Burning`](super::burning::Burning) 组件。
    Burn { duration: f32, dps: f32 },
    /// 在 target 上挂一个 Stunned 组件。
    ///
    /// 首版未实现 —— 占位符，等 Stunned + AI / 输入禁用接入。
    Stun { duration: f32 },
    /// 包装语义：**仅当本次结算 is_crit = true** 才执行内层 trigger。
    ///
    /// 用 `Box` 是因为 enum variant 不能直接持有自身（无限大小）。
    CritOnly(Box<HitTrigger>),
}

/// 标记一个 entity 是某次攻击的判定盒，并携带这次命中的全部规格。
///
/// 跟 [`Hurtbox`](super::Hurtbox) 是对偶身份 marker：hurtbox 回答"谁挨打"，
/// hitbox 回答"谁打人 + 打多少 + 命中后做什么"。
#[derive(Component, Debug)]
pub struct Hitbox {
    /// 攻击发起者 entity —— 用来：
    /// - 自损过滤（[`detect_hitbox_collisions`] 跳过 `hurtbox.owner == hitbox.owner`）；
    /// - [`HitTrigger::Lifesteal`] 回血给 owner；
    /// - 后续做"是谁打死的我"统计 / 击杀归属。
    pub owner: Entity,
    /// 命中规格 —— 见 [`HitSpec`]。
    pub spec: HitSpec,
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
/// 反复发命中消息。
///
/// 一次挥砍可能横跨 3–5 帧，物理 sensor 在这段时间内每帧都看到目标在重叠区里；
/// 如果不去重，一刀变 5 倍伤害。
#[derive(Component, Debug, Default)]
pub struct HitboxHits {
    /// 这里记的是被命中 unit 的 entity（即 `hurtbox.owner`），不是 hurtbox 自己 ——
    /// 一个 unit 可能挂多块 hurtbox（头 / 身 / 腿），但应该算作"同一个人"只打一次。
    pub already_hit: Vec<Entity>,
}

/// "hitbox 撞到 hurtbox" 的最原始事实 —— [`detect_hitbox_collisions`] 发出，
/// [`damage_calc`](super::damage_calc) 消费跑 modifier 流水线再结算。
///
/// **替代了旧版的 `DamageMessage`**：旧消息是"扣多少血给谁"（已经算好了），
/// 本消息是"谁打中了谁"（还没结算）。把"算"这一步独立出来才能塞 modifier。
#[derive(Message, Debug, Clone, Copy)]
pub struct CollisionMessage {
    /// 命中的 hitbox entity —— 下游用 `q_hitbox.get(hitbox)` 取
    /// [`Hitbox`] 拿到 `owner` 和 `spec`。可能在同一帧已被 despawn
    /// （短 lifetime），消费方需优雅 miss。
    pub hitbox: Entity,
    /// 被命中的 unit entity（即 `hurtbox.owner`）。
    pub target: Entity,
}

/// 在世界里 spawn 一块 hitbox，返回它的 entity。
///
/// 调用方负责传：
///
/// - `owner`：攻击发起者；
/// - `faction`：决定走哪一层（[`GameLayer::PlayerHitbox`] / [`GameLayer::EnemyHitbox`]）；
/// - `collider`：形状（盒 / 球 / 胶囊 …）；
/// - `transform`：**世界坐标**位姿（hitbox 不 `ChildOf` owner，调用方自己折算
///   owner 当前 transform）；
/// - `spec`：命中规格，**caster-side 一切已经烧好**（见模块文档）；
/// - `lifetime`：存活秒数。一般跟技能 active 阶段一样长（短 swing）或更长（投射物）。
pub fn spawn_hitbox(
    commands: &mut Commands,
    owner: Entity,
    faction: Faction,
    collider: Collider,
    transform: Transform,
    spec: HitSpec,
    lifetime: f32,
) -> Entity {
    let membership = match faction {
        Faction::Player => GameLayer::PlayerHitbox,
        Faction::Enemy => GameLayer::EnemyHitbox,
    };
    commands
        .spawn((
            Hitbox { owner, spec },
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
/// [`CollisionMessage`]。
///
/// 过滤规则（按出现概率排序）：
/// 1. 对方不是 hurtbox —— 跳过（理论上不会发生，因为物理层 filter 只让 hitbox
///    撞 hurtbox，但保险起见 query miss 直接跳）；
/// 2. `hurtbox.owner == hitbox.owner` —— 自己不打自己；
/// 3. `hitbox.already_hit` 已经记过这个 owner —— 不重复发命中。
///
/// 通过三道之后，把 owner 加进 `already_hit` 并 write 一条 [`CollisionMessage`]。
fn detect_hitbox_collisions(
    mut q_hitbox: Query<(Entity, &Hitbox, &CollidingEntities, &mut HitboxHits)>,
    q_hurtbox: Query<&Hurtbox>,
    mut collisions: MessageWriter<CollisionMessage>,
) {
    for (hitbox_entity, hitbox, colliding, mut hits) in &mut q_hitbox {
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
            collisions.write(CollisionMessage {
                hitbox: hitbox_entity,
                target: hurtbox.owner,
            });
        }
    }
}

/// HitboxPlugin —— hitbox 子系统的注册点。
///
/// 注册：
/// - [`CollisionMessage`]（hitbox×hurtbox 撞到的原始事实）；
/// - [`tick_hitbox_lifetime`]（plain Update，跟 pipeline 解耦 —— 寿命不属于
///   伤害结算流程的一部分）；
/// - [`detect_hitbox_collisions`] 进 [`DamagePipeline::DetectCollision`] set
///   （流水线的第一步）。
pub struct HitboxPlugin;

impl Plugin for HitboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CollisionMessage>()
            .add_systems(Update, tick_hitbox_lifetime)
            .add_systems(
                Update,
                detect_hitbox_collisions.in_set(DamagePipeline::DetectCollision),
            );
    }
}
