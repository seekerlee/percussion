//! Strike —— 一次施法在 active 阶段内的**活态命中判定对象**。
//!
//! # 这个模块解决什么问题
//!
//! 当 [`SkillCast`](super::skill::SkillCast) 进入
//! [`SkillPhase::Active`](super::skill::SkillPhase::Active) 阶段时，需要在一段时间
//! （= [`Skill::active`](super::skill::Skill::active)）内**持续做命中判定**。把"一
//! 次出招"的活态状态对象化成一个 entity = `Strike`，承载：
//!
//! - 谁在打（[`Strike::caster`] + [`Strike::faction`]）
//! - 怎么算命中（[`Strike::effect`] —— 几何参数）
//! - 打中之后做什么（[`Strike::on_hit`] —— [`HitSpec`] modifier 流水线 + effects）
//! - 还能打多久（[`Strike::remaining`]）
//! - 已经打过谁（[`Strike::already_hit`]，per-cast 去重）
//!
//! # 跟 [`SkillCast`](super::skill::SkillCast) 的对偶
//!
//! | | 范围 | 时长 | 谁产生 |
//! |---|---|---|---|
//! | `SkillCast` | windup + active + recovery 整体 | `skill.windup + active + recovery` | `CastSkillRequest` |
//! | `Strike`    | **仅 active 那段**             | `skill.active`                    | `SkillActivatedMessage` |
//!
//! 一次 cast 进入 active 那一帧由桥接模块（`skill_activation`）spawn 一个 `Strike`，
//! 它的 lifetime 跟 active phase 同步推进，归零 despawn。
//!
//! # 为什么不用 avian sensor
//!
//! 以前试过用 collider+sensor 的几何 entity 表达受击，靠 avian 扫重叠 →
//! `CollisionStarted`。限制：调度、顺序、生命周期都卸给物理引擎；spawn /
//! despawn / sensor 事件跨帧顺序难控，调试不直观。改成纯数值之后：
//!
//! - 没有 collider、不进 avian 物理层、不发 sensor 事件
//! - 命中由 [`resolve_strikes`] system 用 **2D XZ 平面**的点 + 半径数学公式算
//! - 单位的"被打中"由 [`HurtRadius`] 标 + 中心点 (`Transform.translation`) 表达
//!
//! 这样物理引擎只剩"占体积、推挤、撞墙"。**damage 完全脱钩**于 avian，跑在
//! Bevy schedule 内，时序可预测、跟 avian API 解耦。
//!
//! # 2D XZ 距离的约定
//!
//! Percussion 是 top-down 自动战斗游戏，Y 高度差几乎只在跳跃时短暂出现。
//! 命中判定**只看 XZ 平面距离**，Y 忽略 —— 跳起来的玩家仍能砍到地面上的怪
//! （否则违反 ARPG 直觉）。所有 `dist(...)` 公式都是
//! `sqrt((dx)² + (dz)²)`，不算 Y。
//!
//! # `AttackEffect` 三类
//!
//! - [`AttackEffect::MeleeReach`] —— 单目标普攻，候选 = 全敌方 unit，每帧从中
//!   选**最近的一个**判 dist。整个 active 期间最多命中 1 次。
//! - [`AttackEffect::SingleTarget`] —— 单目标锁定（cast 时已确定 target），逐帧
//!   检查那个特定 target 的 dist。同样最多命中 1 次。
//! - [`AttackEffect::Aoe`] —— **多目标**圆 / 扇形，候选 = 全敌方 unit，每帧扫
//!   全部，per-target 去重一次性命中（同一 target 在 active 期间只挨一次）。
//!
//! 三类共用同一套"扫候选 → 距离过滤 → 角度过滤（扇形）→ 地空过滤 → 去重 →
//! push 到 `already_hit`"的统一流程，差别只在候选集合与"最近一个 vs 全部"。
//!
//! # 命中下游：发 `HitMessage`
//!
//! 判定到新命中时，本模块发
//! [`HitMessage`](super::hit_data::HitMessage)。`spec` 字段由
//! [`Strike::on_hit`] clone 进消息（caster-side 修正在 spawn 时已烙好），
//! 下游 [`damage_calc`](super::damage_calc) / [`hit_effects`](super::hit_effects)
//! 无需区分来源 —— [`Projectile`](crate::projectile::Projectile) 也发同一种
//! 消息。

use bevy::prelude::*;

use super::DamagePipeline;
use super::hit_data::{Faction, HitMessage, HitSpec};
use super::{Dead, HurtRadius, IsGround};

/// 活态命中判定对象 —— 一次施法在 active 阶段的具象化。
///
/// 详细生命周期 / 对偶关系见模块顶部文档。
#[derive(Component, Debug)]
pub struct Strike {
    /// 攻击发起者 entity。**仅用于命中下游**（effect 系统的吸血回血、击杀
    /// 归属统计），不用于每帧重新读 caster 的位置 / 状态 —— Strike 一旦
    /// spawn，所有 caster-side 数值（[`HitSpec`] modifier 链）都已经烧好，
    /// caster 死了 / 走了 / 状态变了都不影响已飞出的攻击。
    pub caster: Entity,
    /// 攻击者阵营 —— 决定哪些 unit 算"敌方"候选。命中过滤 `target.faction !=
    /// strike.faction`（自己阵营不打自己阵营）。
    pub faction: Faction,
    /// 攻击中心点（**世界坐标**，spawn 时快照）。
    ///
    /// - [`MeleeReach`](AttackEffect::MeleeReach) / [`SingleTarget`](AttackEffect::SingleTarget)：等于 caster 当时的世界坐标。
    /// - [`Aoe`](AttackEffect::Aoe)：可以等于 caster 位置（自爆类）、可以是地图上某点
    ///   （地面 AOE）、可以是投射物落点。
    ///
    /// 一旦 spawn 不再变。Active 期间 caster 移动 / 投射物飞行**不影响**
    /// Strike 的判定中心 —— 跟"caster-side 一切烧在 spawn 那一刻"哲学一致。
    /// 0.05~0.5 秒的 active 时长内 caster 移动量微小，跟随 vs 快照差异极小；
    /// 简单优先选快照。
    pub origin: Vec3,
    /// 几何参数。
    pub effect: AttackEffect,
    /// 命中规格 —— modifier 流水线 + effects。caster-side 修正在
    /// [`recompute_skill_book`](super::skill::recompute_skill_book) 阶段就已经烧
    /// 进 `modifiers`，桥接层只是 clone 进来，本模块不读 / 不改。
    pub on_hit: HitSpec,
    /// 剩余存活时间（秒）。每帧由 [`resolve_strikes`] `-= dt`，归零自动 despawn。
    pub remaining: f32,
    /// 已命中 unit 的去重表。**记的是被命中 unit 的 entity**。一个 unit
    /// 将来可能有多块受击区域（头 / 身 / 腿），但同一次 cast 应该算
    /// "打到同一个人"只命中一次。
    ///
    /// 对单目标 effect（MeleeReach / SingleTarget），此 Vec 非空即代表"本次
    /// 出招已经结束目标判定"—— 余下 active 时间 strike 仍存活但不再尝试命中
    /// 任何 unit（避免敌人 A 被打死之后 strike 自动转打附近的 B）。
    ///
    /// 对 AOE，扇形 / 圆内每个 unit 在 active 期间最多挨一次。
    pub already_hit: Vec<Entity>,
}

/// 几何 / 候选选择策略 —— 三种命中模式。
///
/// 共用基础公式：**两圆相切判定** `dist(origin, target_pos) <=
/// effect_radius + target.hurt_radius`。其中：
///
/// - "`effect_radius`" 对 MeleeReach / SingleTarget 是 `reach`，对 Aoe 是 `radius`。
/// - "`origin`" 由 [`Strike::origin`] 提供（spawn 时快照）。
///
/// 距离一律用 XZ 平面 2D 距离（见模块顶部"2D XZ 距离的约定"）。
#[derive(Debug, Clone)]
pub enum AttackEffect {
    /// 单目标普攻 —— 选**最近的**敌方 unit。整个 active 期间最多命中 1 个目标。
    ///
    /// 算法：候选 = 全敌方 unit；过滤 dist + 角度（无）+ 地空 + 去重；选 dist
    /// 最小者命中。`already_hit` 非空时此 effect 直接跳过判定。
    MeleeReach {
        /// 攻击者的伸臂 / 武器长度（米）。
        reach: f32,
        /// 是否能打到飞行（无 [`IsGround`] marker）的 unit。
        hits_air: bool,
    },
    /// 单目标锁定 —— cast 时已经决定打谁。逐帧检查 dist 是否满足。
    ///
    /// 算法：候选 = `{ target }`；过滤 dist + 地空 + 去重；命中。`already_hit`
    /// 非空时直接跳过。target 已 despawn / 已死 时本帧不命中（active 不退出，
    /// 万一是被同帧前段动作打死也接受）。
    SingleTarget {
        /// 锁定的目标 entity。cast 时由玩家鼠标 / AI 选取 / 法术参数指定。
        target: Entity,
        /// 法术射程（米）。
        reach: f32,
        /// 是否能打到飞行 unit。
        hits_air: bool,
    },
    /// 圆 / 扇形多目标。
    ///
    /// 算法：候选 = 全敌方 unit；过滤 dist + 角度（若 sector 有值）+ 地空 + 去重；
    /// **全部命中**（不挑最近）。同一 target 在 active 期间最多挨一次。
    Aoe {
        /// AOE 半径（米）。
        radius: f32,
        /// 若 `Some`，进一步过滤为扇形（夹角 `2 * half_angle_deg`，对称轴
        /// `facing`）；`None` 即整圆。
        sector: Option<Sector>,
        /// 是否能打到飞行 unit。
        hits_air: bool,
    },
}

/// 扇形过滤参数 —— 在 [`AttackEffect::Aoe`] 的圆形基础上再加一层夹角约束。
///
/// 数学意义上的 "圆扇形 (circular sector)"：圆内被两条半径夹住的那一块。
/// 不叫 `Cone` 是因为：（1）`Cone` 字面 = 3D 圆锥，这里是 XZ 平面 2D 形
/// 状；（2）Bevy 0.18 prelude 已有同名的 3D 几何 primitive，避免重名歧义。
#[derive(Debug, Clone, Copy)]
pub struct Sector {
    /// 扇形对称轴 —— 世界 XZ 平面上的**单位向量**，`.x` 对应世界 X，`.y` 对应
    /// 世界 Z（top-down 投影约定）。spawn 时快照（一般来自 caster 的
    /// [`Facing`](super::facing::Facing) 或某个固定方向）。
    pub facing: Vec2,
    /// 半角度（度）—— 实际命中夹角 = `2 * half_angle_deg`。例如 `30.0` =
    /// 60° 总扇形。
    pub half_angle_deg: f32,
}

/// 推进所有 [`Strike`] 的 lifetime + 跑命中判定 + 发
/// [`HitMessage`].
///
/// 注册在 [`DamagePipeline::DetectHits`] set 内。跟
/// [`Projectile`](crate::projectile::Projectile) 的命中检测同 set，两者发
/// 同一种 `HitMessage`，下游 [`super::damage_calc`] 一视同仁处理。
//
// `clippy::type_complexity`：Bevy Query 参数本来就由 5+ 个类型参数拼成，
// clippy 默认阈值会报。折成 `type` 别名又会触发 invariant lifetime 问题
// （Query 对 D 不变），所以直接 allow，这是 Bevy 社区标准 escape hatch。
#[allow(clippy::type_complexity)]
fn resolve_strikes(
    time: Res<Time>,
    mut commands: Commands,
    mut hits: MessageWriter<HitMessage>,
    mut q_strike: Query<(Entity, &mut Strike)>,
    q_target: Query<
        (Entity, &Transform, &HurtRadius, &Faction, Has<IsGround>),
        (Without<Strike>, Without<Dead>),
    >,
) {
    let dt = time.delta_secs();

    // 一次性收集本帧所有受击候选 —— 不同 strike 共享同一份快照。每帧
    // O(N) 收集，百级 unit 下几 KB 分配，可接受。换来的是：
    //
    // 1. 纯函数 helper 不背 `Query` 的 invariant lifetime 包袱，签名干净。
    // 2. 单元测试可以直接拼 `Vec<TargetData>`，不需要 `World`。
    // 3. 判定逻辑跟 ECS 解耦，未来想换空间索引加速也只动 helper。
    let candidates: Vec<TargetData> = q_target
        .iter()
        .map(|(entity, tf, hr, f, is_ground)| TargetData {
            entity,
            pos: tf.translation,
            hurt_radius: hr.0,
            faction: *f,
            is_ground,
        })
        .collect();

    for (strike_e, mut strike) in &mut q_strike {
        // lifetime 推进。<=0 直接 despawn，本帧不再算命中。
        strike.remaining -= dt;
        if strike.remaining <= 0.0 {
            commands.entity(strike_e).despawn();
            continue;
        }

        // 单目标 effect 已命中 → 本帧跳过（active 仍走到时间归零）。
        let is_single_target = matches!(
            &strike.effect,
            AttackEffect::MeleeReach { .. } | AttackEffect::SingleTarget { .. }
        );
        if is_single_target && !strike.already_hit.is_empty() {
            continue;
        }

        // 跑命中判定 —— 纯函数，输入 strike state + 候选快照，输出新命中。
        let new_hits = judge_hits(&strike, &candidates);

        // 给每个新命中发一条 HitMessage。spec 从 on_hit clone 进消息
        // —— Strike 本身可能在下一帧就 despawn（lifetime 归零），下游不能
        // 依赖 Strike entity 存活。
        for &target in &new_hits {
            hits.write(HitMessage {
                caster: strike.caster,
                target,
                spec: strike.on_hit.clone(),
            });
        }

        // 写回去重表（本 cast 内不再对同一 target 命中）。
        strike.already_hit.extend(new_hits);
    }
}

/// 单个候选 unit 在判定时需要的扁平视图。从 ECS query 一次性 collect 进来，
/// 喂给纯函数 helper —— 让算法跟 `Query` 的 lifetime 包袱解耦，单元测试
/// 也可以直接构造。
#[derive(Debug, Clone, Copy)]
struct TargetData {
    entity: Entity,
    pos: Vec3,
    hurt_radius: f32,
    faction: Faction,
    is_ground: bool,
}

/// 给定一次 strike 的当前状态 + 全候选快照，返回这一帧新命中的 unit entity。
fn judge_hits(strike: &Strike, candidates: &[TargetData]) -> Vec<Entity> {
    match &strike.effect {
        AttackEffect::MeleeReach { reach, hits_air } => judge_nearest_in_circle(
            strike.origin,
            *reach,
            strike.faction,
            *hits_air,
            &strike.already_hit,
            candidates,
        ),
        AttackEffect::SingleTarget {
            target,
            reach,
            hits_air,
        } => judge_single_target(
            strike.origin,
            *target,
            *reach,
            strike.faction,
            *hits_air,
            &strike.already_hit,
            candidates,
        ),
        AttackEffect::Aoe {
            radius,
            sector,
            hits_air,
        } => judge_aoe(
            strike.origin,
            *radius,
            sector.as_ref(),
            strike.faction,
            *hits_air,
            &strike.already_hit,
            candidates,
        ),
    }
}

/// 从候选圆内选**最近**的敌方 unit 返回（至多 1 个）。
fn judge_nearest_in_circle(
    origin: Vec3,
    reach: f32,
    faction: Faction,
    hits_air: bool,
    already_hit: &[Entity],
    candidates: &[TargetData],
) -> Vec<Entity> {
    let mut best: Option<(Entity, f32)> = None;
    for c in candidates {
        if !is_valid_candidate(c, faction, hits_air, already_hit) {
            continue;
        }
        let d2 = xz_distance_sq(origin, c.pos);
        let threshold = reach + c.hurt_radius;
        if d2 > threshold * threshold {
            continue;
        }
        match best {
            None => best = Some((c.entity, d2)),
            Some((_, b)) if d2 < b => best = Some((c.entity, d2)),
            _ => {}
        }
    }
    best.into_iter().map(|(e, _)| e).collect()
}

/// 检查锁定 target 当前是否满足命中条件（候选集 + 阵营 + 地空 + 射程）。
fn judge_single_target(
    origin: Vec3,
    target: Entity,
    reach: f32,
    faction: Faction,
    hits_air: bool,
    already_hit: &[Entity],
    candidates: &[TargetData],
) -> Vec<Entity> {
    // target 已 despawn / 已 Dead / 没受击数据 → 候选集里找不到 → 不命中。
    let Some(c) = candidates.iter().find(|c| c.entity == target) else {
        return Vec::new();
    };
    if !is_valid_candidate(c, faction, hits_air, already_hit) {
        return Vec::new();
    }
    let d2 = xz_distance_sq(origin, c.pos);
    let threshold = reach + c.hurt_radius;
    if d2 <= threshold * threshold {
        vec![c.entity]
    } else {
        Vec::new()
    }
}

/// 圆 / 扇形多目标 —— 全部命中（不挑最近）。
fn judge_aoe(
    origin: Vec3,
    radius: f32,
    sector: Option<&Sector>,
    faction: Faction,
    hits_air: bool,
    already_hit: &[Entity],
    candidates: &[TargetData],
) -> Vec<Entity> {
    let cos_half = sector.map(|c| c.half_angle_deg.to_radians().cos());
    let facing = sector.map(|c| c.facing);

    candidates
        .iter()
        .filter(|c| is_valid_candidate(c, faction, hits_air, already_hit))
        .filter(|c| {
            let d2 = xz_distance_sq(origin, c.pos);
            let threshold = radius + c.hurt_radius;
            d2 <= threshold * threshold
        })
        .filter(|c| match (facing, cos_half) {
            (Some(facing), Some(cos_half)) => in_sector(origin, c.pos, facing, cos_half),
            _ => true,
        })
        .map(|c| c.entity)
        .collect()
}

/// 共享的候选过滤：阵营不同 + 不重复 + 地空规则。
fn is_valid_candidate(
    candidate: &TargetData,
    attacker_faction: Faction,
    hits_air: bool,
    already_hit: &[Entity],
) -> bool {
    if candidate.faction == attacker_faction {
        return false;
    }
    if already_hit.contains(&candidate.entity) {
        return false;
    }
    // 飞行单位只在 hits_air=true 时才被命中。
    if !candidate.is_ground && !hits_air {
        return false;
    }
    true
}

/// XZ 平面距离平方 —— 避免开方，仅与 threshold² 比。
fn xz_distance_sq(a: Vec3, b: Vec3) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx * dx + dz * dz
}

/// 判 `point` 是否在以 `origin` 为顶点、对称轴 `facing`、半夹角 `acos(cos_half)`
/// 的扇形内。
///
/// 退化情况：`point == origin`（距离 0）视为命中 —— 跟自己重合，方向未定义，
/// 但显然在任何扇形内。
fn in_sector(origin: Vec3, point: Vec3, facing: Vec2, cos_half: f32) -> bool {
    let dx = point.x - origin.x;
    let dz = point.z - origin.z;
    let len_sq = dx * dx + dz * dz;
    if len_sq < 1e-12 {
        return true;
    }
    let inv_len = len_sq.sqrt().recip();
    let dot = facing.x * dx * inv_len + facing.y * dz * inv_len;
    dot >= cos_half
}

/// Strike 子系统的注册点。
pub struct StrikePlugin;

impl Plugin for StrikePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            resolve_strikes.in_set(DamagePipeline::DetectHits),
        );
    }
}

#[cfg(test)]
mod tests {
    //! 几何辅助函数的单元测试。Strike → HitMessage 派发等 system 级别
    //! 测试等接通 message 后再加。

    use super::*;

    #[test]
    fn xz_distance_ignores_y() {
        // (1, 100, 1) 到 (4, -50, 5) 的 XZ 距离 = sqrt(3² + 4²) = 5
        let a = Vec3::new(1.0, 100.0, 1.0);
        let b = Vec3::new(4.0, -50.0, 5.0);
        assert_eq!(xz_distance_sq(a, b), 25.0);
    }

    #[test]
    fn in_sector_self_overlap_is_hit() {
        // 跟自己同位置，方向未定，按约定算命中
        let p = Vec3::ZERO;
        assert!(in_sector(p, p, Vec2::X, 0.5));
    }

    #[test]
    fn in_sector_along_axis_is_hit() {
        // 正前方 +X 方向，必然命中（dot=1 >= cos_half 任何 ≤1 的值）
        let origin = Vec3::ZERO;
        let point = Vec3::new(5.0, 0.0, 0.0);
        let facing = Vec2::new(1.0, 0.0);
        let cos_half = 30.0_f32.to_radians().cos();
        assert!(in_sector(origin, point, facing, cos_half));
    }

    #[test]
    fn in_sector_outside_is_miss() {
        // 正侧方 +Z 方向，跟 facing=+X 夹角 90°，超出 30° 半角
        let origin = Vec3::ZERO;
        let point = Vec3::new(0.0, 0.0, 5.0);
        let facing = Vec2::new(1.0, 0.0);
        let cos_half = 30.0_f32.to_radians().cos();
        assert!(!in_sector(origin, point, facing, cos_half));
    }

    #[test]
    fn in_sector_boundary_at_half_angle() {
        // 夹角刚好 = 半角（cos(60°) ≈ 0.5），应该命中（>= 是非严格）
        let origin = Vec3::ZERO;
        // facing = +X，point 在 60° 方向 = (cos60°, sin60°) = (0.5, ≈0.866)
        let point = Vec3::new(0.5, 0.0, 0.866);
        let facing = Vec2::new(1.0, 0.0);
        let cos_half = 60.0_f32.to_radians().cos();
        // dot = cos(夹角) ≈ 0.5；cos_half = 0.5；非严格 >= 命中。
        // 浮点容差小幅放宽
        assert!(in_sector(origin, point, facing, cos_half - 1e-4));
    }

    #[test]
    fn ground_only_attack_skips_air_unit() {
        let attacker = Faction::Player;
        let already_hit: Vec<Entity> = Vec::new();
        let air = TargetData {
            entity: Entity::from_raw_u32(1).unwrap(),
            pos: Vec3::ZERO,
            hurt_radius: 0.3,
            faction: Faction::Enemy,
            is_ground: false,
        };
        // hits_air=false → 跳过
        assert!(!is_valid_candidate(&air, attacker, false, &already_hit));
        // hits_air=true → 命中
        assert!(is_valid_candidate(&air, attacker, true, &already_hit));
    }

    #[test]
    fn same_faction_filtered() {
        let already_hit: Vec<Entity> = Vec::new();
        let ally = TargetData {
            entity: Entity::from_raw_u32(1).unwrap(),
            pos: Vec3::ZERO,
            hurt_radius: 0.3,
            faction: Faction::Player,
            is_ground: true,
        };
        // 同阵营不打
        assert!(!is_valid_candidate(
            &ally,
            Faction::Player,
            true,
            &already_hit
        ));
    }

    #[test]
    fn already_hit_filtered() {
        let attacker = Faction::Player;
        let e = Entity::from_raw_u32(7).unwrap();
        let already_hit = vec![e];
        let dupe = TargetData {
            entity: e,
            pos: Vec3::ZERO,
            hurt_radius: 0.3,
            faction: Faction::Enemy,
            is_ground: true,
        };
        assert!(!is_valid_candidate(&dupe, attacker, true, &already_hit));
    }
}
