//! Burning —— 持续掉血 debuff。
//!
//! 由 [`HitTrigger::Burn`](super::hitbox::HitTrigger::Burn) 在命中时挂到目标
//! 身上。本模块负责每帧扣血 + 到期自动清除。
//!
//! # 为什么不走 damage_calc pipeline
//!
//! Burning 已经是"过去某次命中的衍生物"—— 那次命中的暴击 / 武器倍率 /
//! caster.Strength 在 spawn 时就已经烧进了 Burn 的 dps（trigger 的设计者
//! 自己决定 dps 是 final_amount × x 还是固定值）。所以正在 tick 的 DoT
//! 不应该**再**吃一次 caster-side modifier。
//!
//! 若将来要让 DoT 受 target-side 抗火 / 易燃 buff 影响，更好的做法是让
//! [`tick_burning`] 改发 [`CollisionMessage`](super::hitbox::CollisionMessage)
//! 到一个"虚拟 hitbox" —— 但首版不做。
//!
//! # 在 pipeline 里的位置
//!
//! [`DamagePipeline::PersistentEffects`](super::DamagePipeline::PersistentEffects)
//! 阶段（位于 [`Triggers`](super::DamagePipeline::Triggers) 之后、
//! [`Transition`](super::DamagePipeline::Transition) 之前）。
//! 顺序意义：本帧刚被点燃的目标这一帧不扣 DoT —— Burning 组件是这帧
//! Triggers 阶段才插上的，PersistentEffects 阶段 query 看不到（Bevy 默认
//! 不立即生效 deferred commands 直到 sync point）。这是想要的行为：
//! 点燃当帧只吃命中伤害，下一帧才开始烧。

use bevy::prelude::*;

use super::{DamagePipeline, Dead, Health};

/// 持续掉血的 debuff 组件。
///
/// 由 [`HitTrigger::Burn`](super::hitbox::HitTrigger::Burn) 挂上；
/// [`tick_burning`] 每帧扣血 + 倒计时 + 到期清除。
#[derive(Component, Debug, Clone)]
pub struct Burning {
    /// 剩余持续时间，单位秒。归零或负后下一帧清除。
    pub remaining: f32,
    /// damage per second —— 持续期间每秒扣的血。
    pub dps: f32,
}

/// 每帧推进 Burning：扣血 + 倒计时 + 到期清除。
///
/// `Without<Dead>` —— 死人不再烧（也省得对着尸体扣负血没意义）。
fn tick_burning(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Burning, &mut Health), Without<Dead>>,
) {
    let dt = time.delta_secs();
    for (entity, mut burn, mut hp) in &mut q {
        // 直接扣血 —— 不走 damage_calc（见模块文档）。
        let tick_damage = burn.dps * dt;
        hp.current = (hp.current - tick_damage).max(0.0);

        burn.remaining -= dt;
        if burn.remaining <= 0.0 {
            // 到期清除组件。判死交给 transition_to_dead 在 Transition 阶段统一做。
            if let Ok(mut entity_commands) = commands.get_entity(entity) {
                entity_commands.remove::<Burning>();
            }
        }
    }
}

/// 注册 [`tick_burning`] 到 [`DamagePipeline::PersistentEffects`] set。
pub struct BurningPlugin;

impl Plugin for BurningPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            tick_burning.in_set(DamagePipeline::PersistentEffects),
        );
    }
}
