//! Projectile —— 会动的攻击体。
//!
//! # 这个模块解决什么问题
//!
//! 远程攻击（火球、箭、闪电矢……）的本质是"一发会移动的小球，沿轨迹
//! 飞、撞到第一个敌人就触发命中"。拆开来看是三件正交的事：
//!
//! 1. **命中判定**：球心到敌方 unit 中心的 XZ 距离 ≤ `proj.radius +
//!    hurt_radius` 即命中 → 发
//!    [`CollisionMessage`](crate::unit::hit_data::CollisionMessage) + despawn
//! 2. **轨迹**：每帧"该往哪移动多少"（直线、抛物线、追踪……）
//! 3. **寿命 + 撞墙**：超时 / shape_cast 撞 `GameLayer::Terrain` 即 despawn
//!
//! # 跟 [`Strike`](crate::unit::strike::Strike) 的对偶
//!
//! 两者本质都是"数值命中判定 entity"，差别在：
//!
//! - `Strike`：origin spawn 时快照不变，active 期间持续扫；多/单目标取
//!   决于 effect
//! - `Projectile`：origin 跟着 transform 移动，每帧扫一遍候选，**一发一命中**
//!
//! 所以不复用 `Strike` —— 共享会污染 strike 的"origin 不变"约定。各走各
//! 的 system，都直接发同一种 [`CollisionMessage`]，下游
//! [`damage_calc`](crate::unit::damage_calc) 一视同仁。
//!
//! # 为什么不走 avian sensor
//!
//! 命中检测完全数值化，跟 [`Strike`](crate::unit::strike::Strike) 同样按 XZ
//! 距离公式算；unit 上只有 [`HurtRadius`] 数值、没有 sensor。
//!
//! projectile entity 保留一个 [`Collider`]，但**只**用作
//! [`linear::advance_linear_motion`] 里 `cast_shape` 的 shape 参数（撞墙仍
//! 走 avian），entity 本身不进 broad-phase（没 `Sensor` / `CollisionLayers`
//! / `CollidingEntities`），不产 sensor 事件。
//!
//! # 当前提供
//!
//! - [`Projectile`]：数据型投射物组件（owner / faction / spec / lifetime / radius）
//! - [`spawn_linear_projectile`]：spawn 一发匀速直线投射物的便捷函数
//! - [`ProjectilePlugin`]：注册 [`detect_projectile_hits`] + 轨迹推进
//! - [`linear`] 子模块：直线轨迹（首版唯一）

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::unit::hit_data::{CollisionMessage, Faction, HitSpec};
use crate::unit::{DamagePipeline, Dead, HurtRadius};

pub mod linear;

/// 投射物物理半径（米）。
///
/// 同时用于：
///
/// 1. [`Collider`] 形状 —— 仅供 [`linear::advance_linear_motion`] 里
///    `cast_shape` 算撞墙；
/// 2. [`Projectile::radius`] —— 命中判定圆的半径。
///
/// 两者数值相同；语义独立。如果未来出现"视觉小、判定大"的需求（手感
/// 调优常用招），把 [`Projectile::radius`] 跟 collider 半径解开即可。
const PROJECTILE_RADIUS: f32 = 0.15;

/// 投射物组件 —— 自带"命中下游需要的全部信息"。
///
/// # 数据 vs marker
///
/// `Projectile` 本质就是"会动的命中源"，把 owner / faction / spec / lifetime
/// 都直接烙进本体，不拆子 entity。
///
/// # 字段语义
///
/// 跟 [`Strike`](crate::unit::strike::Strike) 几乎同形，区别仅在 origin
/// 表达：strike origin 是字段（不变），projectile origin = `Transform.translation`
/// （每帧变）。已经在 `Transform` 里就不重复存。
#[derive(Component, Debug)]
pub struct Projectile {
    /// 发射者 entity —— 命中 [`CollisionMessage::caster`]；自伤过滤也读它
    /// （`target == owner` 跳过）。
    pub owner: Entity,
    /// 阵营 —— 跟 [`Strike::faction`](crate::unit::strike::Strike::faction)
    /// 同义，决定"哪些 unit 算敌方"。
    pub faction: Faction,
    /// 命中后果声明（modifiers / triggers）—— caster-side 修正在 spawn 时
    /// 烧好。命中那一帧 clone 进 [`CollisionMessage::spec`]。
    pub spec: HitSpec,
    /// 剩余存活秒数；归零 despawn（无视有没有命中）。
    pub remaining: f32,
    /// 命中判定圆半径（米）。跟 collider 半径独立，便于"视觉 vs 手感"分
    /// 别调参。
    pub radius: f32,
}

/// Spawn 一发匀速直线投射物，返回 entity。
///
/// 内部装出一个独立的 ECS entity：[`Projectile`] 数据 + [`Collider`]（仅
/// 撞墙用）+ [`Transform`] + [`linear::LinearMotion`]。**不**带 sensor /
/// `CollisionLayers` / `CollidingEntities` —— 见模块文档。
///
/// 参数 `damage` / `lifetime` 暂裸传，未来若有真正的远程技能，应该改
/// 成接 [`HitSpec`] 让 skill 系统统一管理（跟近战 `skill_strike` 同款）。
pub fn spawn_linear_projectile(
    commands: &mut Commands,
    owner: Entity,
    faction: Faction,
    position: Vec3,
    velocity: Vec3,
    damage: f32,
    lifetime: f32,
) -> Entity {
    commands
        .spawn((
            Projectile {
                owner,
                faction,
                spec: HitSpec {
                    base_damage: damage,
                    modifiers: Vec::new(),
                    triggers: Vec::new(),
                },
                remaining: lifetime,
                radius: PROJECTILE_RADIUS,
            },
            Collider::sphere(PROJECTILE_RADIUS),
            Transform::from_translation(position),
            linear::LinearMotion(velocity),
        ))
        .id()
}

/// 投射物子系统的注册点。
///
/// - [`detect_projectile_hits`]：每帧 tick lifetime + 命中扫描，命中或超
///   时即 despawn。在 [`DamagePipeline::DetectCollision`] set，跟
///   [`Strike`](crate::unit::strike::Strike) 同位置发 [`CollisionMessage`]。
/// - [`linear::advance_linear_motion`]：直线轨迹推进 + 撞墙销毁。
pub struct ProjectilePlugin;

impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            detect_projectile_hits.in_set(DamagePipeline::DetectCollision),
        )
        .add_systems(
            PostUpdate,
            // 轨迹推进放 PostUpdate.before(Prepare)：跟 movement.rs 的
            // `apply_movement` 同一个时机 —— 在 avian 把 Transform 同步
            // 到 Position 之前写 Transform，下一帧 `detect_projectile_hits`
            // 看到的就是"刚走完这一帧位移"的新位置。
            linear::advance_linear_motion.before(PhysicsSystems::Prepare),
        );
    }
}

/// 每帧推进 projectile 寿命 + 跑命中判定 + 发 [`CollisionMessage`]。
///
/// 一发一命中：第一次命中合法敌方即发 message + despawn，本帧不再扫剩
/// 余候选。同帧多发 projectile 互不影响（per-entity 循环独立）。
///
/// 算法跟 [`crate::unit::strike`] 同款风格："query 一次 collect 候选 →
/// 纯循环判定"，避免 helper 接 `Query` 引起的 invariant lifetime 包袱。
#[allow(clippy::type_complexity)]
fn detect_projectile_hits(
    time: Res<Time>,
    mut commands: Commands,
    mut collisions: MessageWriter<CollisionMessage>,
    mut q_projectile: Query<(Entity, &Transform, &mut Projectile)>,
    q_target: Query<
        (Entity, &Transform, &HurtRadius, &Faction),
        (Without<Projectile>, Without<Dead>),
    >,
) {
    let dt = time.delta_secs();

    // 候选快照 —— 多发 projectile 同帧共享。
    let candidates: Vec<(Entity, Vec3, f32, Faction)> = q_target
        .iter()
        .map(|(e, tf, hr, f)| (e, tf.translation, hr.0, *f))
        .collect();

    for (proj_e, proj_tf, mut proj) in &mut q_projectile {
        // 寿命推进。超时不命中也消失。
        proj.remaining -= dt;
        if proj.remaining <= 0.0 {
            commands.entity(proj_e).despawn();
            continue;
        }

        // 找第一个合法敌方命中 —— 不挑最近，遍历到先撞到的就 break。
        for (target_e, target_pos, target_radius, target_faction) in &candidates {
            if *target_faction == proj.faction {
                continue;
            }
            // 自伤过滤：target == owner（理论上 caster 自己也是敌方阵营之
            // 外，靠 faction 已经拦掉了，这里是双保险）。
            if *target_e == proj.owner {
                continue;
            }
            let dx = proj_tf.translation.x - target_pos.x;
            let dz = proj_tf.translation.z - target_pos.z;
            let d2 = dx * dx + dz * dz;
            let threshold = proj.radius + target_radius;
            if d2 <= threshold * threshold {
                collisions.write(CollisionMessage {
                    caster: proj.owner,
                    target: *target_e,
                    spec: proj.spec.clone(),
                });
                commands.entity(proj_e).despawn();
                break;
            }
        }
    }
}
