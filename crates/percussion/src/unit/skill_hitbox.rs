//! Skill → Hitbox 桥接。
//!
//! # 这个模块解决什么问题
//!
//! [`super::skill`] 子系统只管"技能放出来了"—— 在 active 阶段切入时发一条
//! [`SkillActivatedMessage`] 就完事了，**不**知道 hitbox 是什么。
//! [`super::hitbox`] 子系统只提供 [`spawn_hitbox`] 和"判定撞 hurtbox →
//! 发 [`CollisionMessage`](super::hitbox::CollisionMessage)"，**不**知道 skill 是什么。
//!
//! 中间这块"听到技能激活 → 调用 spawn_hitbox 摆好一块判定盒"的翻译工作
//! 由本模块负责。让两个子系统都保持单向不依赖、各自可测试。
//!
//! # 纯翻译：Spec 里的 caster-side 修正已结清
//!
//! [`HitSpec`](super::hitbox::HitSpec) 里的 modifiers / triggers 在
//! [`SkillBook`](super::skill::SkillBook) recompute 阶段就已经烧进去了
//! （见 [`super::skill::recompute_skill_book`]）。本桥接只负责 clone 一份
//! `on_hit` 丢给 hitbox，**不读任何 caster 数值组件**（[`Strength`](super::Strength) /
//! 未来 buff / equipment）。这样 caster-side 数值业务集中在 recompute 一处，
//! 桥接退化成纯函数。
//!
//! # 未来同源桥接
//!
//! 同一条 [`SkillActivatedMessage`] 将来可能还会有别的订阅者：
//!
//! - skill → 视觉特效（粒子 / 屏幕震 / 命中闪光）
//! - skill → 音效
//! - skill → UI cooldown 飞屏
//!
//! 每个订阅者都应是**独立的桥接模块**（如 `skill_vfx.rs` / `skill_audio.rs`），
//! 不要塞进本文件 —— 本文件的职责就是 "spawn hitbox"。
//!
//! # 坐标转换约定
//!
//! [`SkillEffectKind::MeleeBox`] 的 `offset` 是 **caster-relative** 的：
//!
//! - `offset.x` = 沿 caster facing 方向（正 = 朝前）
//! - `offset.y` = 垂直 facing 方向（正 = facing 左手侧）
//!
//! 转世界坐标时只看 caster 的 [`Facing`]：
//!
//! - `Facing::Right`（朝 +X）：forward = `+X`，左手侧 = `+Z`
//! - `Facing::Left` （朝 -X）：forward = `-X`，左手侧 = `-Z`
//!
//! 因为 `Cuboid` 是对称形状，**只平移盒子中心、不旋转 collider**，
//! 即可在两种 facing 下得到正确的世界盒（盒子在 facing 那一侧延伸 reach
//! 米、在两侧各延伸 swing/2 米）。

use avian3d::prelude::*;
use bevy::prelude::*;

use super::facing::Facing;
use super::hitbox::{Faction, spawn_hitbox};
use super::skill::{SkillActivatedMessage, SkillBook, SkillEffectKind};

/// 桥接 plugin —— 只注册一个 system。
pub struct SkillHitboxPlugin;

impl Plugin for SkillHitboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_hitbox_on_skill_activated);
    }
}

/// 监听 [`SkillActivatedMessage`]，按 [`SkillEffectKind`] 翻译成 hitbox spawn。
///
/// 寿命用 caster 的 [`SkillBook`] 里该技能的 `active` 时长 —— 跟 skill
/// 状态机的 active 阶段同步进退。从 SkillBook 取而不是塞进 message：
/// SkillBook 是 caster 实例化值的 single source of truth，跟"换武器改了
/// active 时长"这种未来场景天然一致；message 越瘦越好。
///
/// caster 中途消失 / 没了 SkillBook / 没了 Faction / SkillBook 找不到该
/// 技能 —— 都直接跳过该条消息，不 panic，但会 `warn!` 出来。这些情况
/// 按设计都不该发生，真发生了说明上游有 bug，需要看到才能修。
///
/// `on_hit` 直接 clone —— 里面的 modifiers / triggers 已被
/// [`recompute_skill_book`](super::skill::recompute_skill_book) 烧好，本桥接
/// 不再添加任何 caster-side 修正。
fn spawn_hitbox_on_skill_activated(
    mut events: MessageReader<SkillActivatedMessage>,
    q_caster: Query<(&Transform, &Facing, &Faction, &SkillBook)>,
    mut commands: Commands,
) {
    for ev in events.read() {
        // 下面两个 `else` 分支按设计都不该走到 —— 一旦走到说明上游有 bug。
        let Ok((caster_tf, facing, faction, book)) = q_caster.get(ev.caster) else {
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

        // `&ev.effect` 借用而不是 move —— SkillEffectKind 不再 Copy。
        match &ev.effect {
            SkillEffectKind::MeleeBox {
                reach,
                swing,
                height,
                offset,
                on_hit,
            } => {
                // facing 决定"前 / 后"和"左 / 右"的世界轴方向。
                let sign = match facing {
                    Facing::Right => 1.0,
                    Facing::Left => -1.0,
                };
                // caster 中心 → hitbox 中心 的世界位移。
                let world_off = Vec3::new(sign * offset.x, 0.0, sign * offset.y);
                let hitbox_tf = Transform::from_translation(caster_tf.translation + world_off);

                spawn_hitbox(
                    &mut commands,
                    ev.caster,
                    *faction,
                    // Cuboid 是对称的，不需要 rotate；中心点放对即可。
                    // 参数顺序 = 全长 (X, Y, Z) = (reach, height, swing)。
                    Collider::cuboid(*reach, *height, *swing),
                    hitbox_tf,
                    on_hit.clone(),
                    skill.active,
                );
            }
        }
    }
}
