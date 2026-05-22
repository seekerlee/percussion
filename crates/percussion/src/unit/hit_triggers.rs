//! Hit triggers —— 命中后的副作用派发器。
//!
//! # 跟 modifier 的区别
//!
//! [`DamageModifier`](super::hitbox::DamageModifier) 影响**伤害数字**（顺序敏感、
//! 串行）；[`HitTrigger`](super::hitbox::HitTrigger) 影响**伤害之外的世界状态**
//! —— 吸血回 caster 血、击退推 target、点燃挂 buff、眩晕挂 debuff……
//!
//! Trigger 之间相互独立（理论上可并发），所以 Vec 里的顺序**不影响结果**。
//! 但实现上仍然顺序遍历（一个 `for` + `match`）：跟 modifier pipeline 同构，
//! 加新 trigger 时只加一个 `match` arm。
//!
//! # 在 pipeline 里的位置
//!
//! 上游 [`damage_calc`](super::damage_calc) 已经写血并发了
//! [`DamageDealtMessage`](super::DamageDealtMessage)。本模块只读这条消息 +
//! hitbox 上的 [`triggers`](super::hitbox::HitSpec::triggers)，把副作用挂上去。
//!
//! # 为什么不把吸血直接写在 damage_calc 里
//!
//! 因为副作用要看 `is_crit`（[`CritOnly`](super::hitbox::HitTrigger::CritOnly)
//! 包装），而 `is_crit` 只有在 modifier 全跑完才知道。所以"跑 modifier → 出
//! is_crit → 派发 trigger"必须分两步。拆成消息之后顺带把"飘字、击杀统计"这种
//! 也能挂在 [`DamageDealtMessage`] 上，不必都堆进一个系统。

use bevy::prelude::*;

use super::burning::Burning;
use super::hitbox::{HitTrigger, Hitbox};
use super::{DamageDealtMessage, DamagePipeline, Health};

/// 派发每条 [`DamageDealtMessage`] 上挂的所有 trigger。
fn dispatch_hit_triggers(
    mut events: MessageReader<DamageDealtMessage>,
    q_hitbox: Query<&Hitbox>,
    mut q_caster_health: Query<&mut Health>,
    mut commands: Commands,
) {
    for ev in events.read() {
        // Hitbox 仍可能在同帧被 despawn（短 lifetime）—— 优雅 miss，跳过
        // 所有 trigger。代价是这次命中不吸血不点燃，可以接受。
        let Ok(hitbox) = q_hitbox.get(ev.hitbox) else {
            continue;
        };

        for trigger in &hitbox.spec.triggers {
            execute_trigger(trigger, ev, &mut q_caster_health, &mut commands);
        }
    }
}

/// 单条 trigger 的执行。提取成独立 fn 是因为
/// [`HitTrigger::CritOnly`] 需要递归调用自己。
fn execute_trigger(
    trigger: &HitTrigger,
    ev: &DamageDealtMessage,
    q_caster_health: &mut Query<&mut Health>,
    commands: &mut Commands,
) {
    match trigger {
        HitTrigger::Lifesteal { ratio } => {
            // 用 ratio 回 caster 血。clamp 到 max 不允许超过血上限。
            // 若 caster 已死（无 Health 或 query 不到），优雅 miss —— 死人不吸血。
            if let Ok(mut hp) = q_caster_health.get_mut(ev.caster) {
                hp.current = (hp.current + ev.final_amount * ratio).min(hp.max);
            }
        }
        HitTrigger::Knockback { .. } => {
            // TODO: 需要 impulse / 速度修正子系统接入后再实现。
            // 占位符让数据结构完整，运行时不做事。
        }
        HitTrigger::Burn { duration, dps } => {
            // 插 / 覆盖 Burning 组件到 target。
            // 重复点燃当前直接覆盖（刷新持续时间 + dps），未来要做"层数"
            // 再扩 Burning 的字段。
            if let Ok(mut entity_commands) = commands.get_entity(ev.target) {
                entity_commands.insert(Burning {
                    remaining: *duration,
                    dps: *dps,
                });
            }
        }
        HitTrigger::Stun { .. } => {
            // TODO: 需要 Stunned 组件 + AI / 输入禁用接入后再实现。
        }
        HitTrigger::CritOnly(inner) => {
            // 包装语义：仅当本次结算是暴击才执行内层 trigger。
            // 用 Box 是因为 enum variant 不能直接持有自身（无限大小）。
            if ev.is_crit {
                execute_trigger(inner, ev, q_caster_health, commands);
            }
        }
    }
}

/// 注册 [`dispatch_hit_triggers`] 到 [`DamagePipeline::Triggers`] set。
pub struct HitTriggersPlugin;

impl Plugin for HitTriggersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            dispatch_hit_triggers.in_set(DamagePipeline::Triggers),
        );
    }
}
