//! Skill → Strike 桥接。
//!
//! # 这个模块解决什么问题
//!
//! [`super::skill`] 子系统只管"技能放出来了"—— 在 active 阶段切入时发一条
//! [`SkillActivatedMessage`] 就完事了，**不**知道命中判定怎么做。
//! [`super::strike`] 子系统只接受"已经摆好的 [`Strike`] entity"，
//! **不**知道 skill 是什么。
//!
//! 中间这块"听到技能激活 → spawn 一个 [`Strike`] entity"的翻译工作由本
//! 模块负责。
//!
//! # `MeleeReach` 几何翻译
//!
//! [`SkillEffectKind::MeleeReach`] 给出的是"沿 facing 朝前一段直线"
//! （`reach` + `offset.x`）。新 [`AttackEffect::MeleeReach`] 是 WC3 / Diablo
//! 风格的"圆形近战射程"：caster 半径内最近的敌方 unit 命中。
//!
//! 翻译规则：
//!
//! - `origin = caster 中心`（不再考虑 offset，圆形对称，前后左右一视同仁）
//! - `radius = offset.x + reach / 2`（直线最远端到 caster 中心的距离 ——
//!   "能够到多远")
//!
//! 这意味着 caster 360° 范围内最近敌人会被打到，背后的敌人也算 —— 这是
//! WC3 单位攻击的典型做法（基本近战不挑前后）。要做"必须正面对敌"再扩
//! 展 [`AttackEffect`] 加扇形锥度 variant。
//!
//! # caster-side 修正已结清
//!
//! [`HitSpec`](super::hit_data::HitSpec) 里的 modifiers / effects 已经被
//! [`recompute_skill_book`](super::skill::recompute_skill_book) 烧进
//! [`SkillBook`](super::skill::SkillBook)，本桥接只 clone 一份 `on_hit` 给
//! [`Strike::on_hit`]，**不读任何 caster 数值组件**。

use bevy::prelude::*;

use super::facing::Facing;
use super::hit_data::Faction;
use super::skill::{SkillActivatedMessage, SkillBook, SkillEffectKind};
use super::strike::{AttackEffect, CandidateSet, Strike};

/// 桥接 plugin —— 只注册一个 system。
pub struct SkillActivationPlugin;

impl Plugin for SkillActivationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_strike_on_skill_activated);
    }
}

/// 监听 [`SkillActivatedMessage`]，按 [`SkillEffectKind`] 翻译成 [`Strike`] spawn。
///
/// 寿命用 caster 的 [`SkillBook`] 里该技能的 `active` 时长 —— 跟 skill 状
/// 态机的 active 阶段同步进退。从 SkillBook 取而不是塞进 message：SkillBook
/// 是 caster 实例化值的 single source of truth，跟"换武器改了 active 时
/// 长"这种未来场景天然一致；message 越瘦越好。
///
/// caster 中途消失 / 没了 SkillBook / 没了 Faction / SkillBook 找不到该技
/// 能 —— 都直接跳过该条消息，不 panic，但会 `warn!` 出来。这些情况按设
/// 计都不该发生，真发生了说明上游有 bug，需要看到才能修。
fn spawn_strike_on_skill_activated(
    mut events: MessageReader<SkillActivatedMessage>,
    q_caster: Query<(&Transform, &Facing, &Faction, &SkillBook)>,
    mut commands: Commands,
) {
    for ev in events.read() {
        // 下面两个 `else` 分支按设计都不该走到 —— 一旦走到说明上游有 bug。
        // `_facing` 暂未使用（圆形 reach 不需要朝向），保留 query 字段是为
        // 了后续加扇形锥度 / 朝向相关 effect 时不用再改 schema。
        let Ok((caster_tf, _facing, faction, book)) = q_caster.get(ev.caster) else {
            warn!(
                "skill activated but caster {:?} missing Transform/Facing/Faction/SkillBook; skipping",
                ev.caster
            );
            continue;
        };
        let Some(skill) = book.get(ev.kind) else {
            warn!(
                "skill {:?} activated on caster {:?} but not in its SkillBook; skipping",
                ev.kind, ev.caster
            );
            continue;
        };

        match &ev.effect {
            SkillEffectKind::MeleeReach {
                reach,
                offset,
                on_hit,
            } => {
                let melee_reach = offset.x + reach / 2.0;

                commands.spawn(Strike {
                    caster: ev.caster,
                    // origin 在 spawn 这一刻 snapshot —— resolve 时不再读
                    // caster transform，技能 active 期间 caster 移动 / 死
                    // 亡都不影响判定位置（"凝固一击"语义）。
                    origin: caster_tf.translation,
                    effect: AttackEffect::MeleeReach {
                        reach: melee_reach,
                        // 暂全部地面攻击。未来给某些 skill 加 "anti-air"
                        // 标记时从 ev.effect / SkillBook 拉出来覆盖。
                        hits_air: false,
                        // 候选集筛子——普通近战只扫敌方。未来 SkillBook 能带「群治
                        // / 群友增益」语义时，这里从 ev.effect / SkillBook 拉 Ally / All 覆盖。
                        candidates: CandidateSet::Hostile(*faction),
                    },
                    on_hit: on_hit.clone(),
                    remaining: skill.active,
                    already_hit: Vec::new(),
                });
            }
        }
    }
}
