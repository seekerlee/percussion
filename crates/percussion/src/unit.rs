//! Unit —— 舞台上的角色身份（敌人 / 佣兵 / 召唤物 / NPC / 玩家共有）。
//!
//! # 这个模块解决什么问题
//!
//! 游戏里会出现各种"角色"：玩家、敌人、佣兵、召唤物、NPC……它们后续要
//! 共享一批通用机制（受伤、死亡、AI 索敌、命中判定、阵营 friend/foe
//! 判定等）。如果各自挂各自的 marker，每加一个机制都得改 N 处 query filter。
//!
//! [`Unit`] 就是这批共享身份的 marker：所有"角色"实体都带它，通用 system
//! 用 `With<Unit>` 一次覆盖全部。
//!
//! # 当前 unit 模块提供
//!
//! - [`Unit`]：身份 marker，所有角色都带
//! - [`Health`]：生命数据，受伤 / 死亡判定的依据
//! - [`Dead`]：marker，标记 unit 处于"死亡状态"。**死 ≠ despawn** —— 死掉
//!   的 entity 还在场上，可以被复活、播放死亡动画、留尸体；什么时候真
//!   销毁是另一刀的事（"尸体清理"，目前没做）。
//! - [`Body`]：marker，声明该 unit 类型"有 body"——参与挡路、不穿障碍物。
//!   配合两个 lifecycle observer（[`disable_body_on_dead`] /
//!   [`reenable_body_on_revive`]）让"尸体不挡路、复活恢复挡路"成为零额外
//!   代码的默认行为。飞行 / 灵体单位**不带**这个 marker。
//! - [`UNIT_BODY_HEIGHT`]：所有 ground unit 共享的 capsule body 总高度常量，
//!   避免不同半径 unit 互推时 Y 方向抖动。
//! - [`facing::Facing`]：单位朝向 component（左 / 右），驱动 sprite
//!   水平镜像，将来供 AI / 技能 / 受击反馈共用。
//! - [`movement`]：Kinematic 移动子系统 —— sweep-and-slide + 重力 + 落地。
//!   提供 [`MoveVelocity`](movement::MoveVelocity) 让"想往哪走"的来源（玩家
//!   输入、AI、击飞……）有地方写。
//! - [`DamageMessage`] / [`UnitDiedMessage`]：受伤 / 死亡的消息总线
//! - model-side system：[`apply_damage_messages`] + [`transition_to_dead`]
//! - lifecycle observer：[`disable_body_on_dead`] + [`reenable_body_on_revive`]
//!
//! # 全局约定：`Without<Dead>` filter
//!
//! 死了的 unit **不应该**继续：受伤、移动、索敌、攻击、被锁定为目标。
//! 因此**所有 unit-level system 默认在 query 上加 `Without<Dead>` filter**，
//! 除非这个 system 明确是处理死亡状态本身（如死亡表演、复活检测）。
//!
//! 这是工程纪律不是类型强制 —— marker 之间没有互斥（写 `apply_damage`
//! 时忘了加 filter，死了的 entity 也会被扣血）。约定写在这里，写 unit
//! 相关 system 时务必想一下"这个 system 对死人合理吗"。
//!
//! 二元 marker 的特殊优势：`Dead` 这一个组件**在 vs 不在**已经能表达
//! 完整的两态，不存在"既生又死"的非法组合。将来如果要引入 `Downed`
//! （倒地但可复活）这种中间态，互斥就需要靠 transition system 统一调度
//! 或者改 enum，那时再说。

use avian3d::prelude::*;
use bevy::prelude::*;

use movement::{MoveVelocity, OnGround};

/// 标记一个 entity 是"角色"。玩家 / 敌人 / 佣兵 / 召唤物 / NPC 都带它。
///
/// 实现 [`Default`] 是为了配合 `#[require(Unit)]` —— 让上层的 marker
/// （如 [`Player`](crate::unit::player::Player)）可以声明“我必然也是 Unit”，
/// spawn 时自动补这个 marker。
#[derive(Component, Debug, Default)]
pub struct Unit;

/// 生命值数据。
///
/// 数据是公开字段，方便 system 直接读写。约定：
///
/// - `current` 永远在 `[0.0, max]` 区间内
/// - `current <= 0.0` **不等于 dead** —— 死亡是 [`Dead`] marker 的存在与否，
///   不是 Health 数值。`current` 归零只是"将要死" 的条件，由
///   [`transition_to_dead`] 在同一帧后段把 [`Dead`] marker 加上去。
///
/// 不实现 [`Default`] 是有意的：每个 unit 的最大血量都是设计决策，不存在
/// "合理默认"。spawn unit 时必须显式 `Health::new(100.0)`。
#[derive(Component, Debug, Clone, Copy)]
pub struct Health {
    /// 当前生命值。
    pub current: f32,
    /// 最大生命值。
    pub max: f32,
}

impl Health {
    /// 满血创建 —— `current == max`。
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }
}

/// 标记一个 unit 处于死亡状态。
///
/// **死 ≠ despawn**。挂上 `Dead` 表示这个 unit "死透了，不再参与战斗"，
/// 但 entity 还在世界里；视觉上可能保留为尸体、播放死亡动画、或者等被
/// 复活技能命中。
///
/// 复活只需要 `commands.entity(e).remove::<Dead>()`（一般还要顺手把
/// `Health::current` 恢复到合理值）。
#[derive(Component, Debug, Default)]
pub struct Dead;

/// 所有 ground unit body 共享的**总高度**（米，含两端半球）。
///
/// 为什么共享：两个不同半径的 capsule 并排相撞，只要它们的总高一致，接
/// 触点就会落在彼此的圆柱中段，接触法线 100% 水平 —— 物理求解器分配的
/// 修正只走 XZ，Y 方向稳定。如果各 unit 自己选高度，矮 unit 顶端与高
/// unit 圆柱段相碰时法线带 Y 分量，会出现"高 unit 被矮 unit 顶起来跳一
/// 下再被重力压下来"的抖动。
///
/// 选 2.0m 是为了**跟当前 sprite 的 2m 高度对齐** —— sprite mesh 中心刚
/// 好等于 body 中心，sprite offset 公式归零，少一个常量。物理含义上 2m
/// 偏高（≈2.05m 人）但 top-down 视角下 Y 看不见，无 gameplay 影响。
///
/// 约束：使用此高度的 capsule，半径 R 必须 **≤ `UNIT_BODY_HEIGHT / 2`**
/// （否则 `length = H - 2R < 0`，capsule 形状无解）。R 超界的"扁宽怪"
/// 要换 shape 并自己处理 Y 推挤副作用，不走这条共享路径。
pub const UNIT_BODY_HEIGHT: f32 = 2.0;

/// 标记一个 unit "有 body"：在物理世界占体积、跟墙碰撞、跟其他带 body
/// 的 unit 互相挡路。
///
/// 这是**存在性**标记 —— marker 在表示"该 unit 类型本来就有 body"；
/// marker 缺席表示该 unit 永久无 body（如飞行单位、灵体单位），不应被
/// `With<Body>` 的物理 / 索敌相关 system 当成实体障碍处理。
///
/// **临时关闭** body（如死后变尸体、被秒杀僵直）走另一条正交路径：往
/// entity 上 insert [`ColliderDisabled`] + [`RigidBodyDisabled`]，avian 在
/// broad-phase 直接跳过、不积分、不生成 contact pair。marker 不动，存在
/// 性不变 —— 区分"永远没 body"和"暂时停用 body"。
///
/// 死亡 → 自动停用 body 的连线由本模块的 [`disable_body_on_dead`] /
/// [`reenable_body_on_revive`] observer 处理；具体 unit 类型只需在自己
/// 的 `#[require(...)]` 链里写上 `Body`、spawn 时手动挂 `Collider` 即可。
///
/// # body 模式：Kinematic + 自主 sweep-and-slide
///
/// 所有带 `Body` 的 unit 都跑 [`movement`] 模块的 Kinematic 移动子系统：
/// 写 [`MoveVelocity`] → `apply_movement` 每帧 sweep-and-slide → 写回
/// `Position`。这样"互相挡 + 挡障碍 + 不被动量推走"是默认行为，**没有**
/// Dynamic 那种"撞一下被甩开"的物理推动效果。详见 [`movement`] 模块顶部
/// 文档。
///
/// `#[require(MoveVelocity, OnGround)]`：spawn 一个带 `Body` 的 entity 时，
/// Bevy 自动补这两个组件（缺一个，movement 系统的 query filter 就不命中），
/// 调用方无需手动挂。
///
/// # 形状约定：capsule 同高
///
/// Ground unit 用 `Collider::capsule(BODY_RADIUS, UNIT_BODY_HEIGHT - 2.0 *
/// BODY_RADIUS)` —— 共享 [`UNIT_BODY_HEIGHT`] 总高度，每个 unit 自定半径。
/// 这样不同体型的 unit 互相挡路时 Y 方向不会抖动（见 [`UNIT_BODY_HEIGHT`]
/// 文档的根因解释）。半径必须 ≤ `UNIT_BODY_HEIGHT / 2`，超出的 unit 要
/// 走自己的 shape 路径。
#[derive(Component, Debug, Default)]
#[require(MoveVelocity, OnGround)]
pub struct Body;

/// 给 unit 造成伤害的消息 —— 任何"伤害源"（近战、投射物、debuff tick、
/// 坠落等）都往这里写，[`apply_damage_messages`] 消费它来扣血。
///
/// 用 [`Message`] 而不是直接改 [`Health`] 是为了**让伤害汇集到一个 system
/// 里处理**：将来要加伤害修正（护甲、易伤、闪避、暴击）只需要改一个
/// 地方；伤害源 system 只负责"我攻击了谁、攻击多少"，不关心结算细节。
#[derive(Message, Debug, Clone, Copy)]
pub struct DamageMessage {
    /// 受伤的 entity。
    pub target: Entity,
    /// 伤害数值（在到达 [`apply_damage_messages`] 时已是最终值）。
    pub amount: f32,
}

/// Unit 死亡通知 —— [`transition_to_dead`] 给某个 unit 挂上 [`Dead`] marker
/// 时发出。
///
/// 让下游 system（特效、掉落、统计、AI 重置等）订阅这个消息接力处理，
/// 而不是各自 polling `Added<Dead>` —— 用 message 把"死亡"建模成事件序列，
/// 后续要做"上一帧死了哪些人"的统计、批处理也方便。
#[derive(Message, Debug, Clone, Copy)]
pub struct UnitDiedMessage {
    /// 死亡的 entity（此时 [`Dead`] marker 已挂上）。
    pub entity: Entity,
}

/// Unit 插件 —— 注册 Health / Dead 相关的数据通路。
///
/// 目前不提供任何视觉表现；视图层（血条、死亡动画、受击闪烁）由各自的
/// 视觉模块独立读 [`Health`] / [`Dead`] / [`UnitDiedMessage`] 来反应。
pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageMessage>()
            .add_message::<UnitDiedMessage>()
            // 顺序：先把所有伤害结算到 Health，再判定谁死了。
            // 否则同一帧"挨打致死"会被推迟一帧才进入 Dead 状态。
            .add_systems(Update, (apply_damage_messages, transition_to_dead).chain())
            // 用 observer（而不是 `Added<Dead>` 的 schedule 内 query）是
            // 为了**同帧响应** —— `transition_to_dead` 通过 Commands insert
            // `Dead`，commands 在 schedule 边界才 flush；observer 在 flush
            // 那一刻原生触发，不依赖 query 跨帧轮询。
            .add_observer(disable_body_on_dead)
            .add_observer(reenable_body_on_revive);
    }
}

/// 消费 [`DamageMessage`]，扣减目标的 [`Health::current`]。
///
/// `Without<Dead>` —— 死人不再受伤（避免重复死亡通知、避免负血溢出）。
/// 如果想做"死后追杀斩"之类效果，那是另一条 message 路径，不走这里。
fn apply_damage_messages(
    mut messages: MessageReader<DamageMessage>,
    mut q_health: Query<&mut Health, Without<Dead>>,
) {
    for msg in messages.read() {
        let Ok(mut health) = q_health.get_mut(msg.target) else {
            // target 已死、不存在、或者根本没有 Health —— 静默忽略。
            // 上游不应假设伤害一定命中（attack 发出去到结算之间可能很多帧）。
            continue;
        };
        health.current = (health.current - msg.amount).max(0.0);
    }
}

/// 把所有 `Health::current <= 0` 且还没挂 [`Dead`] 的 unit 切到死亡状态。
///
/// 同一帧内可能多个 unit 同时死，全部批处理；每个发一条 [`UnitDiedMessage`]
/// 让下游 system 接力。
fn transition_to_dead(
    mut commands: Commands,
    mut died: MessageWriter<UnitDiedMessage>,
    q_health: Query<(Entity, &Health), Without<Dead>>,
) {
    for (entity, health) in &q_health {
        if health.current <= 0.0 {
            commands.entity(entity).insert(Dead);
            died.write(UnitDiedMessage { entity });
        }
    }
}

/// `Dead` 被挂上时，自动给带 [`Body`] 的 unit 停用物理：插入
/// [`ColliderDisabled`] + [`RigidBodyDisabled`]，broad-phase 跳过、不积分。
///
/// 默认行为：尸体不挡路、不再被重力推、不再跟其他 unit 互推。如果将来
/// 想做"尸体仍然挡路"的特定关卡机制，把这条 observer 从 plugin 里摘
/// 掉，或在 unit 上加一个反向 marker 来跳过即可。
///
/// 没有 `Body` marker 的 unit（飞行 / 灵体）直接跳过 —— 它们本来就没
/// body 物理，没什么可停的。
fn disable_body_on_dead(add: On<Add, Dead>, q_body: Query<(), With<Body>>, mut commands: Commands) {
    let entity = add.entity;
    if !q_body.contains(entity) {
        return;
    }
    // 用 `get_entity` 而不是 `entity()`：万一 entity 在同一帧被 despawn，
    // 直接 `entity()` 会 panic；这里宁可静默忽略。
    let Ok(mut ec) = commands.get_entity(entity) else {
        return;
    };
    ec.insert((ColliderDisabled, RigidBodyDisabled));
}

/// `Dead` 被移除时（复活、debug 命令、关卡重置等），把 body 物理恢复
/// —— 配对 [`disable_body_on_dead`]。
///
/// `On<Remove, Dead>` 也会在 entity 被 despawn 时触发（despawn 等同于
/// 所有组件被移除）。此时 entity 已经/即将无效，`get_entity` 失败、
/// `q_body.contains` 也是 false，整条 observer 走空，安全无副作用。
fn reenable_body_on_revive(
    remove: On<Remove, Dead>,
    q_body: Query<(), With<Body>>,
    mut commands: Commands,
) {
    let entity = remove.entity;
    if !q_body.contains(entity) {
        return;
    }
    let Ok(mut ec) = commands.get_entity(entity) else {
        return;
    };
    // 用元组当 bundle 一次性 remove —— 顺序不重要，两个组件互不依赖。
    ec.remove::<(ColliderDisabled, RigidBodyDisabled)>();
}

// ============================================================================
// 子模块：具体的 unit 类型。每种角色（玩家、敌人、佣兵、NPC……）一个
// 独立 module 文件，跟本文件提供的共享身份层（`Unit` / `Health` / `Dead`
// / 伤害消息总线）解耦。新加角色只在 `unit/` 目录里加一个文件并在这里
// 声明即可。
// ============================================================================

pub mod dragon1;
pub mod facing;
pub mod hitbox;
pub mod hurtbox;
pub mod movement;
pub mod player;
pub mod skill;
pub mod skill_hitbox;
