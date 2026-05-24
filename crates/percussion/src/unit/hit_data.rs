//! Hit data —— 一次命中要带的数据 + 命中那一刻发出的信号。
//!
//! # 这个模块解决什么问题
//!
//! 命中检测被切成"产命中事实"和"算后果"两段：
//!
//! - **产**：[`Strike`](super::strike::Strike) / [`Projectile`](crate::projectile::Projectile)
//!   各自做数值距离判定，命中即发 [`CollisionMessage`]。
//! - **算**：[`damage_calc`](super::damage_calc) 跑 [`HitSpec::modifiers`]
//!   流水线 + [`hit_triggers`](super::hit_triggers) 派 [`HitSpec::triggers`]
//!   副作用。
//!
//! 两段中间靠 [`CollisionMessage`] 解耦：消息自包含 spec，**不依赖产源
//! entity 仍存活**。短 lifetime 的 strike entity 可以在判定后立刻消失，
//! 下游 system 依旧能完整结算。
//!
//! # 两阶段后果拆分：modifier vs trigger
//!
//! [`HitSpec`] 把命中后果拆成两条 Vec：
//!
//! - [`modifiers`](HitSpec::modifiers) —— **影响伤害数字**，顺序敏感串行
//!   执行（`Mul(2.0)` 在 `Crit{chance:0.5, mul:2.0}` 之前 vs 之后，最终
//!   值不同）。
//! - [`triggers`](HitSpec::triggers) —— **不影响伤害数字、只挂副作用**，
//!   相互独立（吸血、击退、点燃；顺序原则上无所谓）。
//!
//! 拆开是因为流水线必须先跑完 modifier 才知道最终伤害 + 是否暴击，副作
//! 用里像 [`HitTrigger::CritOnly`] / [`HitTrigger::Lifesteal`] 要读这两个
//! 结果。
//!
//! # caster-side 一切烧在 spawn 那一刻
//!
//! 重要约定：bridge（[`super::skill_activation`] / 投射物 / 陷阱）在 spawn
//! strike / projectile 之前，要把"caster 的 Strength / 武器倍率 / 全局
//! buff"全部读出来、折算成具体的 [`DamageModifier::Mul`] 值塞进
//! [`HitSpec::modifiers`]。
//!
//! 这样命中那一刻不需要再回查 caster（caster 可能已死 / 已 despawn / 状
//! 态变了也不影响），世界状态简洁。代价是 caster 状态变化后已飞出去的
//! 攻击不感知 —— 但这正是想要的行为（射出去的箭不会突然变更狠）。
//!
//! # 友军误伤
//!
//! [`Strike`](super::strike::Strike) / [`Projectile`](crate::projectile::Projectile)
//! 都用 [`Faction`] + `target == caster.owner` 双重过滤（友军不打、自己
//! 不打自己）。当前项目只有一个玩家 + 龙，够用。同侧多 unit 互相误伤问
//! 题等出现需求时再加细分。

use bevy::prelude::*;

/// Unit 的归属阵营 —— [`Strike`](super::strike::Strike) /
/// [`Projectile`](crate::projectile::Projectile) 在命中判定时用它过滤
/// 友/敌目标。
///
/// 当前只有 `Player` / `Enemy` 二元划分。未来要做"派系内对立"
/// （e.g. 召唤物倒戈），再加 variant 或换成 `(group_id, mood)` 等。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Player,
    Enemy,
}

/// "这次命中"的完整声明式规格 —— 由产命中的一方（strike spawn / 投射
/// 物 spawn）填好之后塞进 entity，命中那一刻 clone 进
/// [`CollisionMessage::spec`]。
///
/// 见模块文档 "两阶段后果拆分" + "caster-side 一切烧在 spawn 那一刻"。
#[derive(Debug, Clone)]
pub struct HitSpec {
    /// modifier pipeline 的输入。所有 [`Mul`](DamageModifier::Mul) /
    /// [`Crit`](DamageModifier::Crit) 都从这个值起跑。
    pub base_damage: f32,
    /// 修正流水线 —— 顺序敏感，[`damage_calc`](super::damage_calc) 顺序
    /// 串行执行。
    pub modifiers: Vec<DamageModifier>,
    /// 命中后挂副作用 —— [`hit_triggers`](super::hit_triggers) 顺序遍历
    /// 但互不依赖（顺序不影响结果）。
    pub triggers: Vec<HitTrigger>,
}

/// 影响最终伤害**数值**的修正。
///
/// 多个 modifier 按 Vec 顺序串行：`amount` 从 `base_damage` 起跑，每个
/// modifier 在前一步结果上继续 apply。
#[derive(Debug, Clone, Copy)]
pub enum DamageModifier {
    /// 乘法系数。bridge 把 caster.Strength / 武器倍率 / 全局 buff 加成
    /// 都烧成具体 Mul 值塞进来。多个 Mul 顺序连乘。
    Mul(f32),
    /// 暴击 roll —— `chance` 概率成功；成功则当前 `amount` ×= `mul`、流
    /// 水线输出 `is_crit = true`。多个 Crit 各自独立 roll，任一成功即标
    /// 记 is_crit；倍率连乘。
    Crit { chance: f32, mul: f32 },
}

/// 命中之后挂的副作用。在 damage 已写入目标 Health 之后由
/// [`hit_triggers`](super::hit_triggers) 派发。
#[derive(Debug, Clone)]
pub enum HitTrigger {
    /// 把 `final_amount * ratio` 的血量回给 caster（即
    /// [`CollisionMessage::caster`]）。
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

/// "命中已发生"的最原始事实 —— [`Strike`](super::strike::Strike) /
/// [`Projectile`](crate::projectile::Projectile) 数值判定来源都发它，下
/// 游 [`damage_calc`](super::damage_calc) 消费跑 modifier 流水线再结算。
///
/// 消息**自包含 `spec`**（clone in），不依赖产源 entity 仍存活。这是为
/// 了：
///
/// 1. 让多种产命中源（近战、投射物、未来的陷阱）产同一种消息，下游
///    system 无差别消费；
/// 2. 短 lifetime 的产源（投射物命中即 despawn）在判定后立即消失也不影
///    响下游结算。
#[derive(Message, Debug, Clone)]
pub struct CollisionMessage {
    /// 攻击发起者 —— trigger 系统需要它来回写 caster（吸血加血等）。
    pub caster: Entity,
    /// 被命中的 unit entity。
    pub target: Entity,
    /// 这次命中要走的 spec —— `clone` 进消息，独立于来源 entity 的生命
    /// 周期。拷贝成本：几十字节 Vec（modifiers / triggers），每帧命中数
    /// 十次，可接受。
    pub spec: HitSpec,
}
