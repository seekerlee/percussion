//! Projectile —— 会动的攻击体。
//!
//! # 这个模块解决什么问题
//!
//! 远程攻击（火球、箭、闪电矢……）的本质是"一块带伤害判定的形状沿轨迹
//! 飞行、撞到东西就触发命中"。拆开来看是三件正交的事：
//!
//! 1. **作为攻击体**：判定盒、阵营、自伤过滤、命中发 [`DamageMessage`]
//! 2. **轨迹**：每帧"该往哪移动多少"（直线、抛物线、追踪……）
//! 3. **寿命**：何时消失（命中、超时、撞墙）
//!
//! 第 1 件事跟近战完全一致 —— 都靠 [`Hitbox`](super::unit::hitbox)
//! 子模块的"sensor + `CollidingEntities` + 自动结算" 通路。所以
//! **投射物 entity = `Hitbox` 全套 + [`Projectile`] marker + 轨迹组件**。
//! 这是组合不是继承：判定通路、轨迹通路、寿命通路各自独立 system，
//! 不互相调用。
//!
//! # 为什么不另起一套命中检测
//!
//! 当前 [`Hitbox`](super::unit::hitbox) 用的是 "sensor + `CollidingEntities`
//! 每帧扫重叠"，**没有**做 shape-cast 防穿透。理论上 spec §3.4 要求"高速
//! 投射物不能隧穿"，但首版只有近距离普通速度投射物，先复用 hitbox 通路
//! 简单上路。等真出现"一帧位移 > 目标厚度"的情况，再加 `cast_shape` 防穿
//! 透（轨迹推进 system 里加一段 sweep 即可，命中通路本身不动）。
//!
//! # 命中即销毁
//!
//! 投射物语义是"一发一命中"：碰到第一个合法目标就消失。但 hitbox 子系统
//! 默认是"在 lifetime 内可以多次命中不同目标"（去重在同一 owner，不去重
//! 跨 owner），适合近战横扫。所以 projectile 需要**额外**的"撞了就销毁"
//! 规则 —— [`despawn_on_hit`] 检查 [`HitboxHits::already_hit`] 非空即 despawn。
//!
//! # 跟 terrain 的互动
//!
//! [`Hitbox`](super::unit::hitbox) 默认 filter 只看 [`GameLayer::Hurtbox`]，不会
//! 接触地形。投射物**额外**用 [`SpatialQuery::cast_shape`] 沿这一帧位移路径
//! 扫一段 terrain，撞到就 despawn。这一段顺便也防了 terrain 的隧穿（不会
//! 飞穿墙）。具体实现见各轨迹模块（如 [`linear`]）—— 因为"这一帧打算移
//! 动多少"是轨迹自己的事。
//!
//! # 当前提供
//!
//! - [`Projectile`]：投射物 marker（标记一块 hitbox 是"会动且一发一命中"的）
//! - [`spawn_linear_projectile`]：spawn 一发匀速直线投射物的便捷函数
//! - [`ProjectilePlugin`]：注册 [`despawn_on_hit`] + 轨迹模块的推进 system
//! - [`linear`] 子模块：直线轨迹（首版唯一）

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::unit::hitbox::{Faction, HitboxHits, spawn_hitbox};

pub mod linear;

/// 投射物物理半径（米）。
///
/// 现阶段所有投射物都是同一号小球 —— 视觉表层走 sprite，这里的 collider
/// 只负责与 hurtbox / terrain 的几何判定，不必跟视觉严格一致。这个半径采
/// 取一个能可靠銲中 hurtbox 且不会抱抱着墙角插、又不会在狭缝里卡住的折中
/// 值。未来如果出现【技能不同型号投射物】这种真实需求，再把 collider 开出
/// 去作为参数；现在仅一个 caller（debug 发射键）不抽象。
const PROJECTILE_RADIUS: f32 = 0.15;

/// 标记一个 entity 是投射物（"会动、一发一命中"的攻击体）。
///
/// 跟 [`Hitbox`](super::unit::hitbox::Hitbox) 是 **and** 关系而不是 **or** ——
/// 投射物 entity 一定也带 [`Hitbox`](super::unit::hitbox::Hitbox)，本 marker
/// 只是额外贴一个标签让"命中即销毁"逻辑能 filter 出投射物（避开近战
/// hitbox：那种要在 lifetime 内可以扫多个目标）。
///
/// 不带任何数据 —— 所有"投射物本质上的事"（伤害、阵营、寿命、形状）都
/// 走 [`Hitbox`](super::unit::hitbox::Hitbox) 全套；轨迹细节在
/// [`linear::LinearMotion`] 等轨迹组件上。Projectile marker 自身只回答
/// "这块 hitbox 是不是一发一命中"。
#[derive(Component, Debug, Default)]
pub struct Projectile;

/// Spawn 一发匀速直线投射物，返回 entity。
///
/// # 参数
///
/// - `owner`：发射者 entity —— 命中结算时自伤过滤会读它（见
///   [`Hitbox::owner`](super::unit::hitbox::Hitbox))
/// - `faction`：决定 [`CollisionLayers`] membership（`PlayerHitbox`/`EnemyHitbox`）
/// - `position`：世界坐标里的出膛位置
/// - `velocity`：世界坐标里的速度向量（米/秒）。零向量等于"立刻原地等死亡"，
///   一般避免传零
/// - `damage`：一次命中的伤害
/// - `lifetime`：最长存活秒数；到期即使没命中也 despawn
///
/// # 实现
///
/// 内部走 [`spawn_hitbox`] 拿到全套 hitbox 组件（sensor / 分层 /
/// `CollidingEntities` / 寿命 / 去重 / `Hitbox` 数据），然后再 insert
/// [`Projectile`] marker 和 [`linear::LinearMotion`]。这种"拼起来"的写法
/// 让两个子系统的演化解耦 —— hitbox 子系统改 sensor 行为，本函数不用动；
/// 加一种新轨迹只写新的 `LinearMotion` 等价物，不改这里。
///
/// Collider 固定为半径 [`PROJECTILE_RADIUS`] 的球 —— 见该常量注释。
pub fn spawn_linear_projectile(
    commands: &mut Commands,
    owner: Entity,
    faction: Faction,
    position: Vec3,
    velocity: Vec3,
    damage: f32,
    lifetime: f32,
) -> Entity {
    let entity = spawn_hitbox(
        commands,
        owner,
        faction,
        Collider::sphere(PROJECTILE_RADIUS),
        Transform::from_translation(position),
        damage,
        lifetime,
    );
    commands
        .entity(entity)
        .insert((Projectile, linear::LinearMotion(velocity)));
    entity
}

/// 投射物子系统的注册点。
///
/// - [`despawn_on_hit`]：跨所有轨迹的通用规则 —— 命中即消失
/// - [`linear::advance_linear_motion`]：直线轨迹的位置推进 + terrain sweep 销毁
///
/// 跟 [`HitboxPlugin`](super::unit::hitbox::HitboxPlugin) 配合使用：本插件
/// 假定 `HitboxPlugin` 已经 add，否则 `HitboxHits` / `CollidingEntities`
/// 不会被维护。`lib.rs` 里两者顺序无所谓（都是 Update / PostUpdate 注册
/// 各自 system，不互相依赖 build 期顺序），但 **HitboxPlugin 必须存在**。
pub struct ProjectilePlugin;

impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            // 轨迹推进放 PostUpdate.before(Prepare)：跟 movement.rs 的
            // `apply_movement` 同一个时机——在 avian 把 Transform 同步到
            // Position 之前写 Transform，broad-phase 看到最新位置生成
            // CollidingEntities，下一帧 detect_hitbox_collisions 拿到的
            // 重叠就是这一帧的"投射物到了新位置"的结果。
            linear::advance_linear_motion.before(PhysicsSystems::Prepare),
        )
        // 命中即销毁放 Update：detect_hitbox_collisions 也在 Update 里
        // 写 HitboxHits，本 system 读它。最多延迟一帧 despawn（hitbox 子
        // 系统在 PostUpdate 物理同步后看到接触 → 第 N+1 帧 Update 写
        // HitboxHits → 第 N+1 帧 Update 末尾本 system 看到 → despawn）——
        // 玩法上感受不到。
        .add_systems(Update, despawn_on_hit);
    }
}

/// "命中即销毁"规则：扫所有 [`Projectile`]，发现已经命中过至少一个目标
/// 就 despawn 自身。
///
/// 为什么读 `HitboxHits.already_hit` 而不是订阅 [`DamageMessage`]：
///
/// - `DamageMessage` 是给生命系统消费的，不带 hitbox entity id；订阅它
///   还要反查"哪发投射物干的"
/// - `HitboxHits` 是 hitbox 子系统给每块 hitbox 记的"我命中过谁"，**直接**
///   就是本系统要的信号
///
/// 跟近战 hitbox 区分：近战 hitbox 也会写 `HitboxHits`，但**没有**
/// [`Projectile`] marker，本 query 不命中它们 —— 近战靠 lifetime 自然到期
/// 销毁，可以扫多个目标。
fn despawn_on_hit(q: Query<(Entity, &HitboxHits), With<Projectile>>, mut commands: Commands) {
    for (entity, hits) in &q {
        if !hits.already_hit.is_empty() {
            commands.entity(entity).despawn();
        }
    }
}
