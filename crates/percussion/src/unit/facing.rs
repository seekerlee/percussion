//! 单位朝向 —— 水平二态，驱动 sprite quad 镜像；将来供 AI / 技能 / 受击反馈共用。
//!
//! # 为什么单独成 component
//!
//! 朝向是单位的**领域状态**，不只是渲染细节：
//!
//! - 渲染层：决定 sprite 是否水平镜像
//! - 战斗层（将来）：决定攻击 / 技能的发出方向、命中盒位置
//! - AI（将来）：转向目标
//! - 受击反馈（将来）：被击退后转向加害者
//!
//! 因此放在 unit 公共层，跟具体 unit 实现（player / dragon1 / ...）
//! 解耦。各 unit 用 `#[require(Facing)]` 自动挂上，初始朝向遵循
//! [`Facing::default`]（朝右）。
//!
//! # 为什么二态而不是 4 / 8 向
//!
//! 项目当前所有 sprite sheet 只画了"朝右"姿势，靠水平镜像得到"朝左"。
//! 4 向 / 8 向需要额外的 up / down 帧，目前没有这些资源。等真的需要
//! 顶视区分前后再扩展 —— 届时给 enum 加 Up / Down 变体，所有 match
//! 自然报错提示要补 case，这正是 enum 而非 bool 的好处。

use bevy::prelude::*;

/// 单位朝向：水平二态。
///
/// 各 unit 通过 `#[require(Facing)]` 在 spawn 时自动挂上，初始
/// [`Default::default`] = [`Facing::Right`]，与项目所有 sprite sheet
/// 的原画姿势一致。
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Facing {
    /// 朝右 —— 与 sprite 贴图原画姿势一致，渲染时不镜像。
    #[default]
    Right,
    /// 朝左 —— 渲染时给 sprite quad 加 `Transform.scale.x = -1` 实现
    /// 水平镜像；不动 UV，避免在 atlas sheet 上跨帧采样（见
    /// `crates/percussion/src/unit/player/animation.rs` 中
    /// `tick_player_animation` 的注释）。
    Left,
}
