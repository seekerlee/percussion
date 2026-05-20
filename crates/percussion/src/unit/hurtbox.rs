//! Hurtbox —— unit 的"可被打中"判定盒。
//!
//! # 这个模块解决什么问题
//!
//! 一个 unit 在世界里同时承担两件事：
//!
//! 1. **占体积、被挡 / 挡别人** —— 这是 [`Body`](super::Body) 那个 capsule 干的活
//! 2. **被攻击命中、扣血** —— 这是本模块的 hurtbox 干的活
//!
//! 这两件事如果共用一个 collider 会出问题：
//!
//! - 受击形状想跟着 sprite 变（弯腰 / 倒地受击面变小），body 形状不能跟着变（否则
//!   推挤行为会跟着抽风）；
//! - 一个 unit 将来可能有**多块**受击区（头 / 身 / 腿，不同倍率），body 永远只有一个；
//! - hitbox（武器、子弹）应该穿过 body 直接判定到 hurtbox —— 不能让 body 把
//!   hitbox 弹开。
//!
//! 所以 hurtbox 独立成另一个 entity，作为 body 的子实体：
//!
//! ```text
//! body entity (Player / Dragon1)
//! ├── BillboardSprite child (视觉)
//! └── Hurtbox child (受击判定)
//! ```
//!
//! # 为什么 hurtbox 用 `Sensor`
//!
//! Avian 的 [`Sensor`] 是"只触发接触事件、不参与力学解算"的 collider。我们要的
//! 就是这个语义：被击中要触发"扣血"事件，但不会推开 hitbox / 不会把自己推开。
//! 而且 avian 的 [`MoveAndSlide`] SystemParam 内部 collider query 上挂了
//! `Without<Sensor>` filter，意味着 [`apply_movement`](super::movement::apply_movement)
//! 的 sweep-and-slide **自动**忽略所有 Sensor —— hurtbox 对 body 走路完全透明，
//! 不用我们手写排除。
//!
//! # 为什么 `Hurtbox { owner }` 记 owner 而不是靠父子关系
//!
//! 命中结算时需要知道"打到的这个 hurtbox 属于哪个 unit"来：
//!
//! - 把伤害发到 owner 的 [`Health`](super::Health) 上
//! - 友军误伤判定（"自己的 hitbox 扫到自己的 hurtbox" → 跳过）
//!
//! 走 `ChildOf` 的 parent 链查 owner 在原则上可行，但要：
//!
//! 1. 在命中结算 system 里再加一个 `Query<&ChildOf>` 参数
//! 2. 遍历父链（hurtbox 可能不止一级深，比如挂在 sprite 子实体下）
//! 3. 容忍"父实体已被 despawn"的边角
//!
//! 比起这些，直接在 hurtbox 上记一个 `Entity` 字段简单可靠 —— 命中 system
//! 拿到 hurtbox entity → 读它的 `Hurtbox` 组件 → 拿到 owner → 完事。父子关
//! 系仅用于 Transform 跟随和 despawn 联动，**不**承担逻辑寻址。
//!
//! # 当前 hurtbox 形状跟 body 一致
//!
//! [`spawn_hurtbox`] 让调用方传 `Collider` —— 当前 [`Player`](super::player) /
//! [`Dragon1`](super::dragon1) 都传跟 body 一样的 capsule，简单覆盖整个角色。
//! 等需要"头 / 身 / 腿不同倍率"或"特殊招式让受击面变小"再做：可以多次调
//! [`spawn_hurtbox`] 给一个 owner 挂多块 hurtbox，或者让 hurtbox transform 随
//! sprite 状态机变化。当下不预先抽象。
//!
//! # 命中结算还没接入
//!
//! 本模块只把"hurtbox 这块数据 + 物理 sensor"立起来；真正的"hitbox sensor 命中
//! hurtbox sensor → 发 [`DamageMessage`](super::DamageMessage)"通路要等 hitbox
//! 子系统加进来再写（见 `doc/game-design.md`）。`HurtboxPlugin` 现在只是
//! placeholder，等命中 system 落地时往里加。

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::physics_layers::GameLayer;

/// 标记一个 entity 是某个 unit 的 hurtbox（受击判定盒）。
///
/// 数据极简：只记 `owner` —— 即"这块 hurtbox 代表谁挨打"。命中结算 system 用
/// 它把伤害路由到正确的 unit。
///
/// 形状 / 位置 / 物理 sensor / 分层等其他东西都是 entity 上**别的**组件
/// （[`Collider`] / [`Transform`] / [`Sensor`] / [`CollisionLayers`]），跟 hurtbox 这条
/// 身份信息互不耦合。这种"身份 marker 只带最少数据，物理 / 视觉走 avian / Bevy
/// 自己的组件"的拆分，跟 [`Body`](super::Body) 是一回事。
#[derive(Component, Debug)]
pub struct Hurtbox {
    /// 这块 hurtbox 代表谁挨打。命中结算时把伤害寄到这个 entity 的
    /// [`Health`](super::Health) 上。
    ///
    /// 一般是 hurtbox 的父实体（[`spawn_hurtbox`] 自动把 hurtbox 挂为 `owner` 的
    /// 子实体），但**不强求** —— 没有 ChildOf 关系也行，只要逻辑层愿意维护。
    pub owner: Entity,
}

/// 给一个 unit spawn 一块 hurtbox 子实体，返回 hurtbox entity。
///
/// 调用方负责传 `collider` 形状 + `local` 在 owner 局部坐标里的位置 / 旋转
/// （多数情况下 [`Transform::IDENTITY`] —— 跟 body 完全重合）。同一个 owner
/// 可以多次调本函数挂多块 hurtbox（如头 / 身 / 腿分块），互不冲突。
///
/// # 自动挂上的组件
///
/// - [`Hurtbox { owner }`](Hurtbox)：身份 marker
/// - 传入的 `collider`：形状
/// - [`Sensor`]：声明"只感应不解算"。同时让
///   [`MoveAndSlide`](avian3d::prelude::MoveAndSlide) 的内部 collider query 自动
///   排除本 entity（filter 是 `Without<Sensor>`），body sweep 透明
/// - [`CollisionLayers`]：membership = `Hurtbox`，filter = `[PlayerHitbox, EnemyHitbox]`
///   —— 见 [`crate::physics_layers`] 顶部分层表
/// - 传入的 `local` [`Transform`]：以 owner 为父的局部位姿
/// - [`ChildOf(owner)`](ChildOf)：把 hurtbox 挂为 owner 的子实体，跟随移动 /
///   连带销毁（Bevy 0.18 relationship API）
///
/// # 谁不该用这个函数
///
/// 飞行 / 灵体单位（没 [`Body`](super::Body) 的 unit）目前还没考虑 hurtbox，
/// 等出现"打飞行单位"的场景再扩。
pub fn spawn_hurtbox(
    commands: &mut Commands,
    owner: Entity,
    collider: Collider,
    local: Transform,
) -> Entity {
    commands
        .spawn((
            Hurtbox { owner },
            collider,
            Sensor,
            CollisionLayers::new(
                GameLayer::Hurtbox,
                [GameLayer::PlayerHitbox, GameLayer::EnemyHitbox],
            ),
            local,
            ChildOf(owner),
        ))
        .id()
}

/// HurtboxPlugin —— hurtbox 子系统的注册点。
///
/// 现在是空的：hurtbox 本身没有需要每帧跑的逻辑，[`spawn_hurtbox`] 是按需调用
/// 的工具函数。等 hitbox 子系统接入、要写"hitbox 命中 hurtbox → 发
/// [`DamageMessage`](super::DamageMessage)" 的 system 时，统一注册到这里。
///
/// 把空 plugin 留出来不是"为扩展而扩展"，而是匹配项目里其他子系统（player /
/// dragon1 / movement 都各有 plugin）的注册模式 —— 等加 system 时不用再回头
/// 改 `lib.rs` 的 plugin 元组。
pub struct HurtboxPlugin;

impl Plugin for HurtboxPlugin {
    fn build(&self, _app: &mut App) {
        // 当前没有 system；等命中结算逻辑落地时往这里挂。
    }
}
