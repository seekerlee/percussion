//! Damage calculation —— modifier 流水线 + 写血 + 发结算消息。
//!
//! # 在 pipeline 里的位置
//!
//! 上游：[`strike::resolve_strikes`](super::strike::resolve_strikes) 跟
//! [`projectile::detect_projectile_hits`](crate::projectile::detect_projectile_hits)
//! 在 [`DamagePipeline::DetectCollision`](super::DamagePipeline::DetectCollision)
//! 阶段用数值点-距离判定发出 [`CollisionMessage`](super::hit_data::CollisionMessage)。
//!
//! 本模块在 [`DamagePipeline::ApplyDamage`](super::DamagePipeline::ApplyDamage)
//! 阶段：
//!
//! 1. 读 [`CollisionMessage`](super::hit_data::CollisionMessage) → 拿 caster / target；
//! 2. 读消息里的 [`HitSpec`](super::hit_data::HitSpec) —— `base_damage` 起跑，
//!    沿 `modifiers` 串行跑流水线（每个 [`DamageModifier`](super::hit_data::DamageModifier)
//!    一步），同时记录 `is_crit`；
//! 3. 把最终伤害扣到目标 [`Health::current`](super::Health) 上（clamp 到 0）；
//! 4. 发一条 [`DamageDealtMessage`](super::DamageDealtMessage) —— 后续的
//!    [`hit_triggers`](super::hit_triggers) 派发吸血 / 暴击衍生效果用。
//!
//! 下游：[`hit_triggers`](super::hit_triggers) 在 [`Triggers`](super::DamagePipeline::Triggers)
//! 阶段；[`burning`](super::burning) 等持续 debuff 在
//! [`PersistentEffects`](super::DamagePipeline::PersistentEffects) 阶段独立扣血；
//! 最后 [`transition_to_dead`](super::transition_to_dead) 在
//! [`Transition`](super::DamagePipeline::Transition) 阶段统一判死。
//!
//! # 为什么 modifier 集中跑 / trigger 也集中跑（不拆 N 个 system）
//!
//! Modifier 之间有顺序依赖（Crit 翻倍要乘到 Mul 出来的中间值上），必须**串行**；
//! 拆成 N 个并发 system 反而要靠 message 接力把中间状态串起来，得不偿失。
//! 直接一个 `for` + `match` 的胖函数最简单 —— 加一个新 modifier 就加一个
//! `match` arm。
//!
//! 同理 [`hit_triggers`](super::hit_triggers) 也是一个胖 `match`。区别只是
//! trigger 互不依赖，理论上可以并发，但 N 个 system 各自查消息中的 triggers
//! 找自己关心的 variant 反而是 N 倍 query 开销 —— 还不如一次 match 派发完。
//!
//! # 目标已死怎么办
//!
//! `Without<Dead>` filter 直接 miss → 不写血、不发 DamageDealt。Strike /
//! Projectile 都在自身检测段做了"同目标只命中一次"的去重，但死亡转移
//! 在 Pipeline 最末段才挂 [`Dead`](super::Dead)，所以"命中那一帧目标还活着、
//! 本帧扣完血归零、下一帧可能被另一条 strike / projectile 命中"的窗口
//! 仍然存在。filter 是这窗口的兜底，让"打死了之后再撞不应该再扣"成立。

use bevy::prelude::*;

use super::hit_data::{CollisionMessage, DamageModifier};
use super::{DamageDealtMessage, DamagePipeline, Dead, Health};

/// **纯函数**：给定 base 伤害 + modifier 链 + 取随机数的方式，算出最终
/// 伤害和是否触发暴击。
///
/// # 为什么是纯函数
///
/// 数值策划 / 单元测试 / 未来 deterministic replay 都需要"同输入同输出"。
/// 把"读 ECS + 写 ECS + 发消息"这些副作用全部留在 system 里，[`calc_damage_pipeline`]
/// 只负责把 ECS 数据捞出来、调本函数、把结果灌回 ECS。
///
/// 唯一的不可控来源 —— 随机数 —— 显式作为参数传进来。生产代码传
/// `&mut || fastrand::f32()`，测试可以传 `&mut || 0.0`（必暴击）/
/// `&mut || 1.0`（必不暴）/ 自定义数列。
///
/// # 顺序约定
///
/// `modifiers` 按 Vec 顺序串行执行 —— 顺序敏感（Crit 翻倍乘在之前 Mul
/// 的结果上）。caster-side 的 `Mul`（Strength / 装备 / buff）由 SkillBook
/// recompute 流程 prepend 到链头，所以这里读到的就是"先放大、再暴击"的
/// 自然顺序。
pub fn apply_modifiers(
    base: f32,
    modifiers: &[DamageModifier],
    roll: &mut impl FnMut() -> f32,
) -> (f32, bool) {
    let mut amount = base;
    let mut is_crit = false;
    for modifier in modifiers {
        match modifier {
            DamageModifier::Mul(factor) => {
                amount *= factor;
            }
            DamageModifier::Crit { chance, mul } => {
                // 每次 Crit 独立 roll；多个 Crit 任一成功即记暴击，倍率连乘。
                if roll() < *chance {
                    amount *= mul;
                    is_crit = true;
                }
            }
        }
    }
    (amount, is_crit)
}

/// System：接 ECS 输入、调 [`apply_modifiers`] 算数、写副作用。
///
/// 单条 [`CollisionMessage`] 跑完一次完整 pipeline；多条互相独立。
/// 副作用集中在这里 —— 改 [`Health::current`] + 发 [`DamageDealtMessage`] +
/// 消耗全局 RNG（通过传给 `apply_modifiers` 的闭包）。
///
/// 不再反查任何来源 entity —— `spec` 已经 clone 进
/// `CollisionMessage`，对来源（strike / projectile / 未来 DoT 虚拟来源）
/// 一视同仁。
fn calc_damage_pipeline(
    mut collisions: MessageReader<CollisionMessage>,
    mut q_target: Query<&mut Health, Without<Dead>>,
    mut dealt: MessageWriter<DamageDealtMessage>,
) {
    for ev in collisions.read() {
        // 死人不再被打（见模块文档"目标已死怎么办"）。
        let Ok(mut hp) = q_target.get_mut(ev.target) else {
            continue;
        };

        // 用 fastrand 而不是 bevy_rand：当前只需要无状态的一次性概率，
        // bevy_rand 那套确定性 RNG 资源对单机非联机暂时是过度设计。等
        // 真要做 deterministic replay / 联机同步时，这里换成从 Resource
        // 里取的 Rng 即可，纯函数本身不动。
        let (amount, is_crit) = apply_modifiers(
            ev.spec.base_damage,
            &ev.spec.modifiers,
            &mut || fastrand::f32(),
        );

        // (target-side 修正未来在这里接：armor reduction / vulnerability / 抗性…)

        // 写血 —— clamp 到 0。不允许 `current` 出现负值，简化下游"血量 <= 0
        // 就死"的判定。"过量伤害"这个数值（如果将来要做"斩杀爆头" UI）需要
        // 单独计算并塞进 DamageDealtMessage，不要从 Health.current 反推。
        hp.current = (hp.current - amount).max(0.0);

        // 发结算结果 —— hit_triggers / 飘字 / 击杀统计 / 仇恨表都订阅。
        // triggers clone 进消息，让下游不再依赖来源 entity 存活。
        dealt.write(DamageDealtMessage {
            caster: ev.caster,
            target: ev.target,
            final_amount: amount,
            is_crit,
            triggers: ev.spec.triggers.clone(),
        });
    }
}

/// 注册 [`calc_damage_pipeline`] 到 [`DamagePipeline::ApplyDamage`] set。
pub struct DamageCalcPlugin;

impl Plugin for DamageCalcPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            calc_damage_pipeline.in_set(DamagePipeline::ApplyDamage),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 没有 modifier 时，最终伤害 == base，不暴击。
    #[test]
    fn naked_base_damage_unchanged() {
        let (amount, is_crit) = apply_modifiers(12.0, &[], &mut || 0.5);
        assert_eq!(amount, 12.0);
        assert!(!is_crit);
    }

    /// 多个 Mul 顺序相乘。
    #[test]
    fn multiple_muls_chain() {
        let mods = [DamageModifier::Mul(2.0), DamageModifier::Mul(1.5)];
        let (amount, is_crit) = apply_modifiers(10.0, &mods, &mut || 0.5);
        assert_eq!(amount, 30.0); // 10 * 2 * 1.5
        assert!(!is_crit);
    }

    /// Mul 在前、Crit 在后 —— Crit 倍率乘在 Mul 出来的中间值上。
    #[test]
    fn strength_then_crit() {
        let mods = [
            DamageModifier::Mul(2.0),                       // 力量 / strength
            DamageModifier::Crit { chance: 1.0, mul: 1.5 }, // 必暴
        ];
        let (amount, is_crit) = apply_modifiers(10.0, &mods, &mut || 0.0);
        assert_eq!(amount, 30.0); // 10 * 2 * 1.5
        assert!(is_crit);
    }

    /// Crit chance 边界：`roll < chance` 是严格小于，roll == chance 不暴。
    #[test]
    fn crit_boundary_is_strict_less_than() {
        let mods = [DamageModifier::Crit {
            chance: 0.3,
            mul: 2.0,
        }];
        let (amount, is_crit) = apply_modifiers(10.0, &mods, &mut || 0.3);
        assert_eq!(amount, 10.0);
        assert!(!is_crit);
    }

    /// Crit 不触发（roll 大于 chance）时倍率不应用。
    #[test]
    fn crit_misses_when_roll_too_high() {
        let mods = [DamageModifier::Crit {
            chance: 0.3,
            mul: 5.0,
        }];
        let (amount, is_crit) = apply_modifiers(10.0, &mods, &mut || 0.31);
        assert_eq!(amount, 10.0);
        assert!(!is_crit);
    }

    /// 多个 Crit：任一成功即 `is_crit = true`，倍率连乘。
    #[test]
    fn multiple_crits_chain_multipliers() {
        let mods = [
            DamageModifier::Crit {
                chance: 1.0,
                mul: 2.0,
            },
            DamageModifier::Crit {
                chance: 1.0,
                mul: 1.5,
            },
        ];
        let (amount, is_crit) = apply_modifiers(10.0, &mods, &mut || 0.0);
        assert_eq!(amount, 30.0); // 10 * 2 * 1.5
        assert!(is_crit);
    }

    /// 多个 Crit 一个命中一个不命中：依然 `is_crit = true`。
    #[test]
    fn any_crit_hit_sets_flag() {
        let mods = [
            DamageModifier::Crit {
                chance: 0.5,
                mul: 2.0,
            },
            DamageModifier::Crit {
                chance: 0.5,
                mul: 3.0,
            },
        ];
        // 数列：第一次 0.0（命中），第二次 0.9（不命中）。
        let rolls = [0.0_f32, 0.9_f32];
        let mut idx = 0;
        let (amount, is_crit) = apply_modifiers(10.0, &mods, &mut || {
            let v = rolls[idx];
            idx += 1;
            v
        });
        assert_eq!(amount, 20.0); // 10 * 2
        assert!(is_crit);
    }
}
