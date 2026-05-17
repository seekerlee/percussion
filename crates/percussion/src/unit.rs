//! Unit —— 舞台上的角色身份（敌人 / 佣兵 / 召唤物 / NPC / 玩家共有）。
//!
//! # 这个模块解决什么问题
//!
//! 游戏里会出现各种"角色"：玩家、敌人、佣兵、召唤物、NPC……它们后续要
//! 共享一批通用机制（受伤、死亡、AI 索敌、命中判定、阵营 friend/foe
//! 判定等）。如果各自挂各自的 marker，每加一个机制都得改 N 处 query filter。
//!
//! [`Unit`] 就是这批共享身份的 marker：所有"角色"实体都带它，通用 system
//! 用 `With<Unit>` 一次覆盖全部。
//!
//! # 当前 unit 模块提供
//!
//! - [`Unit`]：身份 marker，所有角色都带
//! - [`Health`]：生命数据，受伤 / 死亡判定的依据
//! - [`Dead`]：marker，标记 unit 处于"死亡状态"。**死 ≠ despawn** —— 死掉
//!   的 entity 还在场上，可以被复活、播放死亡动画、留尸体；什么时候真
//!   销毁是另一刀的事（"尸体清理"，目前没做）。
//! - [`DamageMessage`] / [`UnitDiedMessage`]：受伤 / 死亡的消息总线
//! - 两个 model-side system：[`apply_damage_messages`] + [`transition_to_dead`]
//!
//! # 全局约定：`Without<Dead>` filter
//!
//! 死了的 unit **不应该**继续：受伤、移动、索敌、攻击、被锁定为目标。
//! 因此**所有 unit-level system 默认在 query 上加 `Without<Dead>` filter**，
//! 除非这个 system 明确是处理死亡状态本身（如死亡表演、复活检测）。
//!
//! 这是工程纪律不是类型强制 —— marker 之间没有互斥（写 `apply_damage`
//! 时忘了加 filter，死了的 entity 也会被扣血）。约定写在这里，写 unit
//! 相关 system 时务必想一下"这个 system 对死人合理吗"。
//!
//! 二元 marker 的特殊优势：`Dead` 这一个组件**在 vs 不在**已经能表达
//! 完整的两态，不存在"既生又死"的非法组合。将来如果要引入 `Downed`
//! （倒地但可复活）这种中间态，互斥就需要靠 transition system 统一调度
//! 或者改 enum，那时再说。

use bevy::prelude::*;

/// 标记一个 entity 是"角色"。玩家 / 敌人 / 佣兵 / 召唤物 / NPC 都带它。
///
/// 实现 [`Default`] 是为了配合 `#[require(Unit)]` —— 让上层的 marker
/// （如 [`Player`](crate::player::Player)）可以声明"我必然也是 Unit"，
/// spawn 时自动补这个 marker。
#[derive(Component, Debug, Default)]
pub struct Unit;

/// 生命值数据。
///
/// 数据是公开字段，方便 system 直接读写。约定：
///
/// - `current` 永远在 `[0.0, max]` 区间内
/// - `current <= 0.0` **不等于 dead** —— 死亡是 [`Dead`] marker 的存在与否，
///   不是 Health 数值。`current` 归零只是"将要死" 的条件，由
///   [`transition_to_dead`] 在同一帧后段把 [`Dead`] marker 加上去。
///
/// 不实现 [`Default`] 是有意的：每个 unit 的最大血量都是设计决策，不存在
/// "合理默认"。spawn unit 时必须显式 `Health::new(100.0)`。
#[derive(Component, Debug, Clone, Copy)]
pub struct Health {
    /// 当前生命值。
    pub current: f32,
    /// 最大生命值。
    pub max: f32,
}

impl Health {
    /// 满血创建 —— `current == max`。
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }
}

/// 标记一个 unit 处于死亡状态。
///
/// **死 ≠ despawn**。挂上 `Dead` 表示这个 unit "死透了，不再参与战斗"，
/// 但 entity 还在世界里；视觉上可能保留为尸体、播放死亡动画、或者等被
/// 复活技能命中。
///
/// 复活只需要 `commands.entity(e).remove::<Dead>()`（一般还要顺手把
/// `Health::current` 恢复到合理值）。
#[derive(Component, Debug, Default)]
pub struct Dead;

/// 给 unit 造成伤害的消息 —— 任何"伤害源"（近战、投射物、debuff tick、
/// 坠落等）都往这里写，[`apply_damage_messages`] 消费它来扣血。
///
/// 用 [`Message`] 而不是直接改 [`Health`] 是为了**让伤害汇集到一个 system
/// 里处理**：将来要加伤害修正（护甲、易伤、闪避、暴击）只需要改一个
/// 地方；伤害源 system 只负责"我攻击了谁、攻击多少"，不关心结算细节。
#[derive(Message, Debug, Clone, Copy)]
pub struct DamageMessage {
    /// 受伤的 entity。
    pub target: Entity,
    /// 伤害数值（在到达 [`apply_damage_messages`] 时已是最终值）。
    pub amount: f32,
}

/// Unit 死亡通知 —— [`transition_to_dead`] 给某个 unit 挂上 [`Dead`] marker
/// 时发出。
///
/// 让下游 system（特效、掉落、统计、AI 重置等）订阅这个消息接力处理，
/// 而不是各自 polling `Added<Dead>` —— 用 message 把"死亡"建模成事件序列，
/// 后续要做"上一帧死了哪些人"的统计、批处理也方便。
#[derive(Message, Debug, Clone, Copy)]
pub struct UnitDiedMessage {
    /// 死亡的 entity（此时 [`Dead`] marker 已挂上）。
    pub entity: Entity,
}

/// Unit 插件 —— 注册 Health / Dead 相关的数据通路。
///
/// 目前不提供任何视觉表现；视图层（血条、死亡动画、受击闪烁）由各自的
/// 视觉模块独立读 [`Health`] / [`Dead`] / [`UnitDiedMessage`] 来反应。
pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageMessage>()
            .add_message::<UnitDiedMessage>()
            // 顺序：先把所有伤害结算到 Health，再判定谁死了。
            // 否则同一帧"挨打致死"会被推迟一帧才进入 Dead 状态。
            .add_systems(Update, (apply_damage_messages, transition_to_dead).chain());
    }
}

/// 消费 [`DamageMessage`]，扣减目标的 [`Health::current`]。
///
/// `Without<Dead>` —— 死人不再受伤（避免重复死亡通知、避免负血溢出）。
/// 如果想做"死后追杀斩"之类效果，那是另一条 message 路径，不走这里。
fn apply_damage_messages(
    mut messages: MessageReader<DamageMessage>,
    mut q_health: Query<&mut Health, Without<Dead>>,
) {
    for msg in messages.read() {
        let Ok(mut health) = q_health.get_mut(msg.target) else {
            // target 已死、不存在、或者根本没有 Health —— 静默忽略。
            // 上游不应假设伤害一定命中（attack 发出去到结算之间可能很多帧）。
            continue;
        };
        health.current = (health.current - msg.amount).max(0.0);
    }
}

/// 把所有 `Health::current <= 0` 且还没挂 [`Dead`] 的 unit 切到死亡状态。
///
/// 同一帧内可能多个 unit 同时死，全部批处理；每个发一条 [`UnitDiedMessage`]
/// 让下游 system 接力。
fn transition_to_dead(
    mut commands: Commands,
    mut died: MessageWriter<UnitDiedMessage>,
    q_health: Query<(Entity, &Health), Without<Dead>>,
) {
    for (entity, health) in &q_health {
        if health.current <= 0.0 {
            commands.entity(entity).insert(Dead);
            died.write(UnitDiedMessage { entity });
        }
    }
}
