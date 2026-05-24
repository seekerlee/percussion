//! 直线匀速轨迹 —— 首版唯一的投射物运动方式。
//!
//! # 这个模块负责什么
//!
//! `advance_linear_motion` 做两件事：
//!
//! 1. **位置推进** —— `transform.translation += velocity * dt`
//! 2. **撞墙销毁** —— shape cast 探到 [`GameLayer::Terrain`] 就 despawn
//!
//! 投射物 entity 的销毁触发点分散在三个地方，各管一种：
//!
//! | 触发条件 | 代码位置 | 备注 |
//! |---|---|---|
//! | lifetime 倒计时归零 | [`super`]（`projectile.rs`） | `detect_projectile_hits` 每帧扣 `remaining` |
//! | XZ 距离命中敌方单位 | [`super`]（`projectile.rs`） | `detect_projectile_hits` 命中后发消息 + despawn |
//! | 撞 terrain（墙） | 本模块 | 这里的 `advance_linear_motion` |
//!
//! 命中敌方这条线走 [`super`] 是因为它是数值点-距离判定（跟轨迹无关）；
//! 撞墙走本模块是因为它要靠轨迹该帧的位移 sweep。
//!
//! # 撞墙判定为什么归轨迹模块
//!
//! "这一帧打算移动多少"是轨迹特有的事 —— 直线就是 `velocity * dt`，抛物
//! 线还要叠重力分量。撞墙的 shape cast 起点 / 方向 / 距离全部依赖这个
//! 信息，所以必然跟具体轨迹绑定。等加抛物线 / 追踪轨迹时，每种轨迹自己
//! 写一份 sweep；不抽公共 helper，因为不同轨迹的输入差异大，硬抽反而难懂。
//!
//! # 为什么用 `cast_shape` 而不是 `cast_ray`
//!
//! 投射物自己是有体积的（sphere / cuboid），跟墙的接触是"它的外壳碰到
//! 墙"，不是"它的中心点碰到墙"。`cast_ray` 等价于把投射物当成一个 0 体积
//! 的点扫，会"撞墙前一格还差半个半径才停"；`cast_shape` 用投射物自身的
//! collider 扫，命中点是外壳贴上墙的瞬间。视觉对得上。

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::physics_layers::GameLayer;

/// 匀速直线运动：每帧位移 = `velocity * dt`。
///
/// 内部公开字段 —— 当前没有"修改 velocity 时要同步做别的事"的需求，直接
/// 暴露 `Vec3` 最简单。等有了"改 velocity 要发消息 / 触发动画"再封装。
///
/// 跟 [`MoveVelocity`](crate::unit::movement::MoveVelocity) 是平行概念但语
/// 义不同：那个是"unit 想往哪走的期望速度"，sweep-and-slide 会修改它；
/// 这个是"投射物当前的飞行速度"，飞行期间不变（直线轨迹的本质）。所以
/// 它们**不**复用同一个组件 —— 共享会导致 movement 模块的 `apply_gravity`
/// 给投射物加重力（投射物不该有重力），或者 sweep-and-slide 把投射物撞
/// 墙后弹开（投射物撞墙应该消失）。
#[derive(Component, Debug)]
pub struct LinearMotion(pub Vec3);

/// 每帧推进位置，并沿路径扫一段 terrain。
///
/// 流程：
/// 1. 算这一帧的位移 `delta = velocity * dt`
/// 2. 用 [`SpatialQuery::cast_shape`] 从当前位置朝 `delta` 方向扫，距离
///    为 `delta.length()`，filter 仅 [`GameLayer::Terrain`]
/// 3. 命中 → 立即 despawn（不更新 transform —— 投射物视觉上停在出发位置
///    那一帧，然后消失）
/// 4. 未命中 → `transform.translation += delta`，让 avian 在
///    [`PhysicsSystems::Prepare`] 阶段把 Transform 同步到 Position
///
/// # 跳过 `velocity ≈ 0` 的 entity
///
/// `Dir3::new` 对零向量返回 `Err`。直接 `unwrap` 会 panic。零速度的投射物
/// 没意义但 spawn 调用方失误可能传出来，这里 silent skip + 不更新位置
/// （反正 0 * dt = 0），等 lifetime 到期自然销毁。
fn cast_terrain(
    spatial_query: &SpatialQuery,
    shape: &Collider,
    origin: Vec3,
    delta: Vec3,
    entity: Entity,
) -> Option<ShapeHitData> {
    let dir = Dir3::new(delta).ok()?;
    let config = ShapeCastConfig::from_max_distance(delta.length());
    let filter =
        SpatialQueryFilter::from_excluded_entities([entity]).with_mask([GameLayer::Terrain]);
    spatial_query.cast_shape(shape, origin, Quat::IDENTITY, dir, &config, &filter)
}

pub(super) fn advance_linear_motion(
    time: Res<Time>,
    spatial_query: SpatialQuery,
    mut q: Query<(Entity, &Collider, &mut Transform, &LinearMotion)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, collider, mut transform, motion) in &mut q {
        let delta = motion.0 * dt;
        if delta.length_squared() == 0.0 {
            continue;
        }
        if cast_terrain(
            &spatial_query,
            collider,
            transform.translation,
            delta,
            entity,
        )
        .is_some()
        {
            // 撞 terrain：立即销毁，不更新 transform（见函数文档）。
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += delta;
    }
}
