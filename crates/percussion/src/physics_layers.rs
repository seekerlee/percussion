//! 物理碰撞分层 —— 全局唯一的 [`GameLayer`] 枚举 + 每种 entity 的 membership /
//! filter 约定。
//!
//! # 这个模块解决什么问题
//!
//! 世界里同时存在几种"形状不同、关心对象也不同"的 collider：
//!
//! - body capsule（unit 自己占体积，挡路）
//! - 地形 cuboid（stage 屏障）
//! - 投射物的 collider（仅用作 [`SpatialQuery::cast_shape`] 的 shape 参数，
//!   不进 broad-phase；见 [`crate::projectile`]）
//!
//! 如果让它们都默认互相碰，会出现一堆"语义反常"的接触 —— 比如 body 把另一
//! 个 body 当 sensor 推、或者 body 跟所有地形都触发整盘屏障接触。
//!
//! Avian 用 [`CollisionLayers`] 做"按身份过滤"：每个 collider 声明自己**是**
//! 哪一层（`memberships`），又**想看**哪些层（`filters`）。两个 collider 互相
//! 看到（双向都看）才会生成接触对。本模块只负责定义层、不写过滤规则 ——
//! 规则在使用方（[`crate::stage`] / [`crate::unit`] 各 spawn 点）就地写出来，
//! 哪个 collider 该看见谁一眼能看到。
//!
//! # 命中判定为什么不在这里
//!
//! 攻击命中 / 受击范围**完全不走 avian sensor**：unit 上挂的是
//! [`HurtRadius`](crate::unit::HurtRadius) 数值字段，
//! [`Strike`](crate::unit::strike::Strike) /
//! [`Projectile`](crate::projectile::Projectile) 每帧用 XZ 距离做点-球判定。
//! 所以本枚举里**没有** `Hurtbox` / `Hitbox` 层 —— 命中判定跟物理 broad-phase
//! 解耦，分层规则只服务"形体碰撞 / 移动 sweep"。
//!
//! # 当前的层（按位顺序）
//!
//! | 序号 | Variant   | 谁属于这一层                                | 谁想看到这一层  |
//! |------|-----------|---------------------------------------------|-----------------|
//! | 0    | `Default` | 没显式配过 `CollisionLayers` 的（fallback） | —               |
//! | 1    | `Terrain` | stage 的地面 + 5 面屏障                     | Body            |
//! | 2    | `Body`    | unit capsule                                | Body, Terrain   |
//!
//! `Default` 占第 0 位是 avian 官方示例的惯例：没显式配 [`CollisionLayers`] 的
//! collider 默认落在它上面、跟所有层都互相看见。这给"还没接入分层"的 collider
//! 一个安全的兜底层，避免漏配一处 collider 就静默地变成"穿模幽灵"。
//!
//! # 拓展原则
//!
//! 新加 collider 类型时：先想清楚"它代表什么身份、想看哪些身份"，再决定要不
//! 要新增层。能复用现有层（如 npc 也用 `Body`）就不要拆，层数膨胀会让过滤
//! 表难维护。
//!
//! 真要加：在枚举尾部追加新 variant —— 不要在中间插，那会让所有现存 collider
//! 的 bit 位偏移、改变 saved scene 的语义。
//!
//! # 为什么放 crate root 而不是 `unit/`
//!
//! 多个模块共用：`stage` 也要给屏障配 layer。放 `unit/` 下会让 stage 反向依
//! 赖 unit，颠倒。crate root 是它的自然位置。

use avian3d::prelude::*;

/// 全局物理分层。`#[derive(PhysicsLayer)]` 由 avian 提供，把 enum variant 编码
/// 成位掩码（variant 0 = bit 0 = 0b1，variant 1 = bit 1 = 0b10，依此类推），
/// 让 [`CollisionLayers::new`] 既能接 `GameLayer` 值也能接 `[GameLayer; N]`
/// 数组（avian 实现了 `From<L> for LayerMask` 和 `From<[L; N]> for LayerMask`）。
///
/// 必须实现 [`Default`] —— avian `PhysicsLayer` trait 要求；`#[default]` 标在
/// `Default` 上即可。每层的具体语义见模块顶部表格。
#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum GameLayer {
    /// 兜底层：没配 [`CollisionLayers`] 的 collider 默认在这里，跟所有层互相
    /// 看见。spawn 时漏配不会让 collider 变成"隐形"，方便定位。
    #[default]
    Default,
    /// 地形：stage 的地面 + 4 面立面屏障 + 顶面屏障。只跟 [`Body`](Self::Body)
    /// 互相看。
    Terrain,
    /// Unit body 的物理 capsule。看见同类（unit 互相挡）+ 地形（被墙挡）。
    Body,
}
