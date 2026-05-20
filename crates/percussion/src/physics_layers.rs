//! 物理碰撞分层 —— 全局唯一的 [`GameLayer`] 枚举 + 每种 entity 的 membership /
//! filter 约定。
//!
//! # 这个模块解决什么问题
//!
//! 加入 hurtbox / hitbox 之后，世界里同时存在好几种"形状不同、关心对象也不同"
//! 的 collider：
//!
//! - body capsule（unit 自己占体积，挡路）
//! - 地形 cuboid（stage 屏障）
//! - hurtbox sensor（受击判定盒）
//! - hitbox sensor（攻击判定盒）
//!
//! 如果让它们都默认互相碰，会出现一堆"语义反常"的接触：
//!
//! - body 把 hurtbox 当墙撞 → 角色脚下飘起来
//! - body 互相挤压时 hitbox sensor 误触发"自己打到自己"
//! - hurtbox 把另一个 hurtbox 当墙 → 两个 sensor 互推没意义
//!
//! Avian 用 [`CollisionLayers`] 做"按身份过滤"：每个 collider 声明自己**是**
//! 哪一层（`memberships`），又**想看**哪些层（`filters`）。两个 collider 互相
//! 看到（双向都看）才会生成接触对。本模块只负责定义层、不写过滤规则 ——
//! 规则在使用方（[`crate::stage`] / [`crate::unit`] 各 spawn 点）就地写出来，
//! 哪个 collider 该看见谁一眼能看到。
//!
//! # 当前的层（按位顺序）
//!
//! | 序号 | Variant         | 谁属于这一层                     | 谁想看到这一层                  |
//! |------|-----------------|----------------------------------|---------------------------------|
//! | 0    | `Default`       | 没显式配过 `CollisionLayers` 的（fallback） | —                       |
//! | 1    | `Terrain`       | stage 的地面 + 5 面屏障          | Body                            |
//! | 2    | `Body`          | unit capsule                     | Body, Terrain                   |
//! | 3    | `Hurtbox`       | unit 的受击 sensor               | PlayerHitbox, EnemyHitbox       |
//! | 4    | `PlayerHitbox`  | 玩家方攻击 sensor（未实现）      | Hurtbox                         |
//! | 5    | `EnemyHitbox`   | 敌方攻击 sensor（未实现）        | Hurtbox                         |
//!
//! `Default` 占第 0 位是 avian 官方示例的惯例：没显式配 [`CollisionLayers`] 的
//! collider 默认落在它上面、跟所有层都互相看见。这给"还没接入分层"的 collider
//! 一个安全的兜底层，避免漏配一处 collider 就静默地变成"穿模幽灵"。
//!
//! # 友军误伤为什么不靠分层
//!
//! 玩家攻击 = `PlayerHitbox`、玩家自身受击 = `Hurtbox`，按表 `PlayerHitbox` ↔
//! `Hurtbox` 是互相看的；意味着**玩家挥剑会扫到自己的 hurtbox**。这是有意为之 ——
//! 分层只解决"哪些 collider 物理上能接触"，"友 / 敌"是 gameplay 决策。
//! [`Hurtbox`](crate::unit::hurtbox::Hurtbox) 上记了 `owner`，命中结算时检查
//! `hurtbox.owner == hitbox.owner` 跳过即可，单层 `Hurtbox` 足够。
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
//! 多个模块共用：`stage` 也要给屏障配 layer，未来的 projectile / pickup 也都
//! 会用。放 `unit/` 下会让 stage 反向依赖 unit，颠倒。crate root 是它的自然位置。

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
    /// **不**看 hurtbox / hitbox —— 那些是判定盒，body 不该被它们影响。
    Body,
    /// Unit 受击判定 sensor。看见 [`PlayerHitbox`](Self::PlayerHitbox) +
    /// [`EnemyHitbox`](Self::EnemyHitbox)。同侧 / 异侧的过滤由 gameplay 层
    /// 用 [`Hurtbox::owner`](crate::unit::hurtbox::Hurtbox::owner) 检查。
    Hurtbox,
    /// 玩家方攻击 sensor（武器挥击、子弹等）。只看 [`Hurtbox`](Self::Hurtbox)。
    /// 现阶段未实现，留着待用。
    PlayerHitbox,
    /// 敌方攻击 sensor。同 [`PlayerHitbox`](Self::PlayerHitbox)，只看
    /// [`Hurtbox`](Self::Hurtbox)。
    EnemyHitbox,
}
