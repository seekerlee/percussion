//! 物理碰撞体 wireframe 可视化 —— 开发时把 `Collider` 画出来。
//!
//! 本模块由 `crate::dev` 在 `#[cfg(debug_assertions)]` 下统一编译。release
//! 构建里整个 `dev` 模块都不存在，零运行时开销。
//!
//! # 这个模块解决什么问题
//!
//! 当前 [`Body`](crate::unit::Body) capsule + [`Terrain`](crate::physics_layers::GameLayer::Terrain)
//! cuboid 是 unit / 关卡的物理形体。摆错位置 / 形状不对，运行时屏幕上什么
//! 都看不到 —— 全靠肉眼推断挡路 / 推挤是否生效。开发期把这些 collider
//! 的 wireframe 画出来直接看。
//!
//! # 为什么直接用 avian 自带的 [`PhysicsDebugPlugin`]
//!
//! avian3d 自带的调试渲染器用 Bevy `Gizmos`（线段图元）画每种 `Collider`
//! 形状的 wireframe，3D 形状（cuboid / sphere / capsule / 三角网格 /
//! 凸包）全覆盖、跟着 `GlobalTransform` 实时更新。自己写一遍纯属重造轮子
//! —— 而且 avian 的形状-画法映射表是跟着 collider 实现走的，自己写还得
//! 追着升级。
//!
//! `debug-plugin` 已经在 avian3d 的 `default` features 里，所以
//! [`PhysicsDebugPlugin`] 的代码本来就编进了二进制，**只是没注册**。
//!
//! # 跟自己写 gizmo system 的对比
//!
//! 写 `Query<(&Collider, &GlobalTransform)>` 的 system 自己画 gizmo ——
//! 暂不做。理由：
//!
//! 1. avian 已经画了，重复工作；
//! 2. 颜色 / 开关定制可以走 avian 现成的
//!    [`PhysicsGizmos`](avian3d::prelude::PhysicsGizmos)（GizmoConfigGroup
//!    单独配置，不跟其他 gizmo group 互相影响）和
//!    [`DebugRender`](avian3d::prelude::DebugRender) per-entity 组件 ——
//!    将来要按层染色时给 spawn 路径加 `DebugRender` 即可，不动 debug 渲染
//!    本身；
//! 3. 染色逻辑要加的时候放 dev 模块里写"`Added<某 marker>` → 给 entity
//!    挂 `DebugRender`"的 system，领域代码不需要知道 dev 工具存在。
//!
//! # 默认行为
//!
//! avian 的默认配置：所有 collider wireframe 画成橙色，AABB / 接触点 /
//! raycast 默认关。视觉噪声只来自"我这帧有几个 collider"，加新东西不会
//! 出现意料外的额外可视化。要改默认色 / 开 AABB / 开 raycast 可视化时，
//! 在 `build()` 里改 `PhysicsGizmos` 配置 —— 现在不预先做，等真有需求。

use avian3d::prelude::PhysicsDebugPlugin as AvianPhysicsDebugPlugin;
use bevy::prelude::*;

/// 把 avian 自带的物理碰撞体 wireframe 调试渲染挂上 App。
///
/// 命名跟 avian 的 `PhysicsDebugPlugin` 区分（带 `Avian` 前缀的是 alias），
/// 让 `lib.rs` 里 dev 工具列表的 plugin 名跟所在模块路径对得上：
/// `dev::physics_debug::PhysicsDebugPlugin`。
//
// 临时在 `lib.rs` 里注释掉了注册，dead_code 警告会让 clippy `-D warnings`
// 爆。短期"看视觉效果"用，回头开回来就可以删 allow。
#[allow(dead_code)]
pub struct PhysicsDebugPlugin;

impl Plugin for PhysicsDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(AvianPhysicsDebugPlugin);
    }
}
