//! Kinematic 移动 —— sweep-and-slide + 重力 + 落地，覆盖所有 [`Body`] unit。
//!
//! # 这个模块解决什么问题
//!
//! 切到 [`RigidBody::Kinematic`] 之后（见 [`Body`] 文档），avian solver
//! **不会**自动碰 Kinematic body 的 `Position` / `LinearVelocity`：
//!
//! 1. 不会自动改 `Position` —— 想让 unit 走起来必须代码自己写
//! 2. 不会自动 depenetrate —— 穿模了 solver 也不修
//! 3. 不会施加重力 —— avian `Gravity` 系统的 filter 是 Dynamic only
//!
//! 这三件事原本"为我们做了"的部分，现在都得我们做。本模块用 avian 提供
//! 的 [`MoveAndSlide`] SystemParam（基于 `cast_shape` 的官方实现）把
//! 这三件事接管掉，对外暴露成一个朴素的"写 [`MoveVelocity`] → unit 走起来"
//! 的接口。
//!
//! # 数据流
//!
//! ```text
//! [input / AI / 击飞 / ...]  →  写 MoveVelocity.xz   ─┐
//!                                                       ├→ apply_movement
//! [apply_gravity]            →  写 MoveVelocity.y   ─┘    │
//!                                                          ↓
//!                                          调 MoveAndSlide::move_and_slide
//!                                                          │
//!                                          ┌───────────────┴────────────┐
//!                                          ↓                            ↓
//!                                  写 Position（沿接触面 slide）  更新 OnGround
//! ```
//!
//! # 为什么不复用 `LinearVelocity`
//!
//! avian 的位置集成器 `integrate_positions` 对 Kinematic body 也会执行
//! `Position += LinearVelocity * dt`（只有积分速度的那一步会跳过 Kinematic，
//! 位置积分一视同仁）。我们用 `move_and_slide` 自己写 `Position`；如果
//! 同时往 `LinearVelocity` 写一份，每帧位移会被加两次。
//!
//! 解法是隔离一个独立组件 [`MoveVelocity`]：所有"想动"的来源写它而非
//! `LinearVelocity`，`LinearVelocity` 永远为 0，集成器加 0 等于没事。
//!
//! # 不属于本模块
//!
//! - 输入读取 / AI / 击飞 —— 它们写 [`MoveVelocity`]，本模块只消费
//! - 摩擦 / damping —— 还没需求（unit 没有"地面阻力"的玩法），加进来再说

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{Body, Dead};
use crate::physics_layers::GameLayer;

/// 重力加速度（米/秒²）。
///
/// 25 而不是地球的 9.8 —— ARPG 游戏感的惯例值，落地利落、不拖沓。如果将来
/// 出现"水下减重"之类玩法再抽 `Gravity` resource，现在写死避免猜测性扩展。
const GRAVITY: f32 = 25.0;

/// "脚下站着东西"的判定阈值：接触法线的 Y 分量大于这个值才算地面。
///
/// 0.7 ≈ cos(45°) —— 45° 以内的斜坡可以站立 / 行走，更陡的算墙。当前关卡
/// 只有平地，但留着供将来的斜面 / 阶梯使用。
const GROUND_NORMAL_Y_MIN: f32 = 0.7;

/// unit "想往哪走"的速度（米/秒）。
///
/// 各种"想动"的来源（玩家输入、AI、重力、击飞 impulse、跳跃……）都写它或
/// 加到它上面。每帧 [`apply_movement`] 把它当作期望速度喂给
/// [`MoveAndSlide::move_and_slide`]，再用返回的 `projected_velocity` 写回这里
/// —— 让 slide 之后的剩余速度自然延续到下一帧（比如撞墙后沿墙滑的方向）。
///
/// 见模块顶部 "为什么不复用 `LinearVelocity`" 一节解释为什么独立成组件。
#[derive(Component, Debug, Default)]
pub struct MoveVelocity(pub Vec3);

/// 上一帧 [`apply_movement`] 是否检测到脚下站着东西。
///
/// 主要给 [`apply_gravity`] 用：站在地上时不再往 `MoveVelocity.y` 累积重力，
/// 避免两个副作用 ——
///
/// 1. vy 持续累积成 -1000 之类的大负值，某帧一旦脱离地面（被击飞 / 走下台阶）
///    速度暴跌看起来很违和
/// 2. 每帧花一次 shape-cast 撞同一片地，纯浪费
#[derive(Component, Debug, Default)]
pub struct OnGround(pub bool);

/// MovementPlugin —— 注册 [`apply_gravity`] + [`apply_movement`]。
///
/// schedule = `PostUpdate` before `PhysicsSet::Prepare`，跟 avian
/// `examples/kinematic_character_3d` 一致：在 Update 阶段所有写入
/// [`MoveVelocity`] 的 system（玩家输入、AI 等）都跑完之后、avian 把 collider
/// 同步到 broad-phase 之前，统一应用。
pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (apply_gravity, apply_movement)
                .chain()
                .before(PhysicsSystems::Prepare),
        );
    }
}

/// 把重力累积到 `MoveVelocity.y`。
///
/// `OnGround` 时不累积，且如果 vy 还残留负值就强制清零，避免站立时被微小残
/// 余速度往地里挤。Y 方向之外不动 —— 重力跟 XZ 速度正交。
///
/// `Without<Dead>` 为了尸体不再被重力累积 Y 速度 —— 同是遵守 unit 模块顶部
/// 那条全局约定；[`apply_movement`] 同样过滤，理由见那里。
//
// `clippy::type_complexity`：跟 [`apply_movement`] 同款，理由见那里。
#[allow(clippy::type_complexity)]
fn apply_gravity(
    time: Res<Time>,
    mut q: Query<(&mut MoveVelocity, &OnGround), (With<Body>, Without<Dead>)>,
) {
    let dt = time.delta_secs();
    for (mut vel, grounded) in &mut q {
        if grounded.0 {
            if vel.0.y < 0.0 {
                vel.0.y = 0.0;
            }
        } else {
            vel.0.y -= GRAVITY * dt;
        }
    }
}

/// 主移动循环：对每个 [`Body`] unit 跑一次 [`MoveAndSlide::move_and_slide`]。
///
/// 把 `MoveVelocity * dt` 当作期望位移，shape-cast 试探沿途接触，沿接触面 slide，
/// 最终把 slide 之后的位置写回 `Transform.translation`，剩余速度写回
/// [`MoveVelocity`]，并刷新 [`OnGround`]。
///
/// # 为什么走 `Transform` 而不是 avian 的 `Position`
///
/// [`MoveAndSlide`] 内部的 [`SpatialQuery`] 已经声明了对 `Position` 的只读访问
/// （`Query<&Position, ..>`）。如果外层 query 再写 `&mut Position`，Bevy
/// scheduler 静态检测判定"写集"与"读集"在 Body 实体上重叠 → 启动期 panic
/// （B0001）。`Transform` 不属于 avian 任何 SystemParam 的查询范围，写它零冲突。
///
/// avian 在 [`PhysicsSystems::Prepare`] 阶段会做 `Transform → Position` 同步，
/// 所以"写 Transform.translation"就等同于"设置 avian 位置"。这是 avian 官方
/// `examples/move_and_slide_3d.rs` 和 `examples/kinematic_character_3d/plugin.rs`
/// 都在用的模式。
///
/// # 为什么要 `Without<Dead>`
///
/// 表面上仅是遵守 unit 模块顶部的全局约定，但对本 system 尤其重要：死亡时
/// [`disable_body_on_dead`](super::disable_body_on_dead) 会给本 entity 挂
/// [`ColliderDisabled`]，**别人**的 spatial query 就看不见这块 collider 了。但
/// [`MoveAndSlide::move_and_slide`] 是拿本 entity 的 `Collider` 当 sweep 形状传进
/// 去的普通函数，不看 `ColliderDisabled`；它内部的 `depenetrate` 一查，看到
/// 别人（活的玩家）当前与本 entity 重叠（因为玩家 sweep 看不见 disabled
/// collider，直接走进了尸体），就把**本 entity 自己**推开。视觉上是"尸体被玩
/// 家推走"，实际是尸体自己跑这个 system + depenetration 把自己推开。锁这
/// 条 filter 后尸体不再跑本 system，bug 从根上消失。
//
// `clippy::type_complexity`：Bevy Query 参数本来就由 5–7 个类型参数拼成，
// clippy 默认阈值到一个稍复杂的 system 就会报。另折成 `type` 别名反而
// 让调用点看不出 filter，这是 Bevy 社区的标准 escape hatch。
#[allow(clippy::type_complexity)]
fn apply_movement(
    time: Res<Time>,
    mover: MoveAndSlide,
    mut q: Query<
        (
            Entity,
            &Collider,
            &mut Transform,
            &mut MoveVelocity,
            &mut OnGround,
        ),
        (With<Body>, Without<Dead>),
    >,
) {
    let config = MoveAndSlideConfig::default();
    let dt = time.delta();
    for (entity, collider, mut transform, mut vel, mut grounded) in &mut q {
        let mut hit_ground = false;
        // 排除自身：shape-cast 从自己的 collider 位置出发，不排除会立即"命中"自己。
        //
        // mask 限定只看 Body / Terrain 两层：hurtbox / hitbox 都是 Sensor，
        // 逻辑上不该拦住 body 走路。虽然 [`MoveAndSlide`] 内部 collider query
        // 上挂了 `Without<Sensor>`（深入物体的 depenetration 的那一路会自动跳
        // sensor），但外部 shape-cast 依然会被 filter mask 控制 —— 不明示限
        // 层的话，扫过去还是会命中 hurtbox sensor 、返回 hit 事件。带上这句
        // 让 sweep 从源头上就忽略它们。
        let filter = SpatialQueryFilter::from_excluded_entities([entity])
            .with_mask([GameLayer::Body, GameLayer::Terrain]);
        let out = mover.move_and_slide(
            collider,
            transform.translation,
            transform.rotation,
            vel.0,
            dt,
            &config,
            &filter,
            |hit| {
                // 法线指向被撞物体的外表面，对地面来说就是朝上。Y 分量足够大
                // 就把"踩在地面上"标志立起来。
                if hit.normal.y >= GROUND_NORMAL_Y_MIN {
                    hit_ground = true;
                }
                MoveAndSlideHitResponse::Accept
            },
        );
        transform.translation = out.position;
        // projected_velocity = slide 之后剩余的速度。保留它让下一帧的 gravity /
        // input 在合理基线上继续叠加，避免"撞墙弹回原速"的不自然感。
        vel.0 = out.projected_velocity;
        grounded.0 = hit_ground;
    }
}
