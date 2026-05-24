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
//! - [`DamageDealtMessage`] / [`UnitDiedMessage`]：伤害结算 / 死亡的消息总线
//! - model-side system：[`damage_calc`] / [`hit_triggers`] / [`transition_to_dead`]
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

/// 受击半径 —— **damage 视角**下该 unit 的圆盘大小（米）。
///
/// 这是命中判定算 `dist(attacker, target) <= attacker.reach + target.hurt_radius`
/// 时右边那一项。**跟 [`Body`] capsule 的几何半径是不同概念**，不要混淆:
///
/// | 概念 | 回答的问题 | 谁来读 |
/// |---|---|---|
/// | `Body` capsule | 我占多大体积、能不能挤过去、撞不撞墙 | avian 移动 / 推挤 / 撞墙 |
/// | `HurtRadius` | 我多大范围算"被命中"（数值化圆盘表征） | strike resolve 算法 |
///
/// 初始数值可以跟 body capsule 半径相同（视觉一致），但**演化路径完全独立**：
/// 比如想给"满血龙"一个比 body 更大的受击面（更易被远程命中、平衡用），
/// 或者反过来给"灵活刺客"小受击面而 body 不变，都只动这个值，不动 body
/// collider；反之巨型怪要更难推过去（大 body）但受击面不变（HurtRadius
/// 不变），物理 / 数值各调各的，互不污染。
///
/// 飞行 / 地面单位都需要 `HurtRadius` —— 受击半径跟"在不在地上"无关；
/// 地空判定走 [`IsGround`] marker 单独标。
#[derive(Component, Debug, Clone, Copy)]
pub struct HurtRadius(pub f32);

/// 标记 unit 是**地面单位**。飞行 / 灵体单位**不**带这个 marker。
///
/// 用于"对地 / 对空"技能过滤：技能数值里带一个 `hits_air: bool`，strike
/// resolve 算法据此选择是否命中没有 `IsGround` 的 unit。当前 percussion 是
/// top-down 自动战斗刷子，地空只做**离散二分** —— 不做高度细分，也不做
/// "扫腿不扫头"那种纵向精度。
///
/// 跟 [`Body`] / [`HurtRadius`] 都正交：飞行单位可以有 [`Body`]（占体积，
/// 不撞墙的飞兵也得占空），可以有 `HurtRadius`（要被打中），但**没有**
/// `IsGround`。
///
/// 选 marker 而不是 `Tier(Ground | Air)` enum 是因为：
/// 1. 当前只有"地"vs"非地"二分，enum 二分等价 marker 在不在；
/// 2. marker 直接在 query filter（`With<IsGround>` / `Without<IsGround>`）里用，
///    比 enum match 顺手；
/// 3. 真出现"水下""攀墙"等第三态时再升级 enum，简单到那时再说。
#[derive(Component, Debug, Default)]
pub struct IsGround;

/// 角色的"攻击力"系数 —— 让 [`Strike`](strike::Strike) /
/// [`Projectile`](crate::projectile::Projectile) spawn 时把 caster 的整体输出
/// 系数烙进 [`HitSpec`](hit_data::HitSpec) 的 modifier 流水线里（详见
/// [`skill_activation`](crate::unit::skill_activation) 的 bridge system）。
///
/// 这是个**示例性**的 caster-side 通用 stat —— 真正决定一个角色"打多痛"
/// 的远不止这一个 stat（武器加成、状态加成、buff、暴击率……）。目前
/// 只用 `Strength` 一项是为了：
///
/// 1. 验证"caster-side 在 spawn 时就把所有自身修正烧进 spec"这条架构原则；
/// 2. 让 Player 跟 Dragon 走同一条流水线，参与同样的 damage 计算。
///
/// 缺席视为 1.0（无加成）。
#[derive(Component, Debug, Clone, Copy)]
pub struct Strength(pub f32);

/// 一次完整命中**结算后**发出的消息 —— [`damage_calc`] 跑完 modifier 流水线、
/// 把最终伤害写入目标 [`Health`] 之后发。
///
/// 用它做下游 hook：trigger 派发（[`hit_triggers`]，吸血 / 暴击衍生效果）、
/// 飘字 UI、击杀统计、AI 仇恨表。
///
/// 替代了旧版的 `DamageMessage` —— 旧消息是"我请求扣血"，本消息是"血
/// 已经扣完了"。语义反转：从"伤害源声明"→"权威结算结果"，后段 system
/// 拿到就直接用，不用再担心 race / 重复结算。
///
/// **自包含 `triggers`**（clone in），让 [`hit_triggers`] 无需反查来源
/// entity。原因详见 [`CollisionMessage`](hit_data::CollisionMessage) 同款理由。
#[derive(Message, Debug, Clone)]
pub struct DamageDealtMessage {
    /// 攻击发起者。trigger 系统需要它来回写 caster（吸血加血）。
    pub caster: Entity,
    /// 受伤的 entity。
    pub target: Entity,
    /// 最终扣掉的血量（被 `(current - amount).max(0.0)` clamp 之前的值；
    /// 吸血等比例 trigger 应该按这个算）。
    pub final_amount: f32,
    /// 这次结算 modifier 流水线是否触发了暴击。`CritOnly` trigger 用它
    /// 判定是否启动条件分支。
    pub is_crit: bool,
    /// 命中后挂的 trigger 列表 —— 从来源 spec clone 进来。`hit_triggers`
    /// 直接遍历，不再查 [`Strike`](strike::Strike)。
    pub triggers: Vec<hit_data::HitTrigger>,
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

/// Damage pipeline 的执行阶段 —— 用 [`SystemSet`] 把跨模块的 system 排
/// 成一条流水线，单点定义顺序，比每个 system 各自 `.before()/.after()`
/// 链清晰得多。
///
/// 设计哲学："命中检测→伤害结算→trigger 派发→死亡转移" 是一条逻辑
/// 上不可乱序的流水线。每段产物喂下一段消费，并发性能不是这里的瓶颈
/// （单帧整条 < 1ms），所以**顺序优先于并发**。
///
/// 各 set 由对应 module 自家 plugin 把自己的 system 塞进去；本模块只
/// 负责"把这 5 个 set 排成一条链"。新增阶段在这里加一个变体，并在
/// `chain()` 元组里放到正确位置即可。
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DamagePipeline {
    /// [`strike`] / [`crate::projectile`] 按 XZ 距离扫出命中，发
    /// [`CollisionMessage`](hit_data::CollisionMessage)。
    DetectCollision,
    /// [`damage_calc`] 跑 modifier 流水线，把最终伤害扣到
    /// [`Health`]，发 [`DamageDealtMessage`]。
    ApplyDamage,
    /// [`hit_triggers`] 按 [`DamageDealtMessage`] 派发 per-hit triggers
    /// （吸血 / 衍生效果），可能进一步修改 caster / target 状态。
    Triggers,
    /// [`burning`] 等"持续 debuff tick"模块跑自己的周期性扣血 ——
    /// 跟 per-hit trigger 区分开（trigger 是"一次命中触发的副作用"，
    /// 持续 debuff 是"已存在的状态每帧 tick"）。
    PersistentEffects,
    /// 本模块的 [`transition_to_dead`] —— 扫 Health 归零的，挂 Dead。
    /// 必须放最后：让本帧所有扣血来源（pipeline + DoT）都结算完才判死。
    Transition,
}

/// Unit 插件 —— 注册 Health / Dead 相关的数据通路。
///
/// 目前不提供任何视觉表现；视图层（血条、死亡动画、受击闪烁）由各自的
/// 视觉模块独立读 [`Health`] / [`Dead`] / [`UnitDiedMessage`] 来反应。
pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<hit_data::CollisionMessage>()
            .add_message::<DamageDealtMessage>()
            .add_message::<UnitDiedMessage>()
            // 把整条 damage pipeline 的 5 个 set 串成一条链。各 set 内部
            // 由对应 module 的 plugin 自家 system 填充。`chain()` 让相
            // 邻 set 之间满足 happens-before。
            .configure_sets(
                Update,
                (
                    DamagePipeline::DetectCollision,
                    DamagePipeline::ApplyDamage,
                    DamagePipeline::Triggers,
                    DamagePipeline::PersistentEffects,
                    DamagePipeline::Transition,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                transition_to_dead.in_set(DamagePipeline::Transition),
            )
            // 用 observer（而不是 `Added<Dead>` 的 schedule 内 query）是
            // 为了**同帧响应** —— `transition_to_dead` 通过 Commands insert
            // `Dead`，commands 在 schedule 边界才 flush；observer 在 flush
            // 那一刻原生触发，不依赖 query 跨帧轮询。
            .add_observer(disable_body_on_dead)
            .add_observer(reenable_body_on_revive);
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

pub mod burning;
pub mod damage_calc;
pub mod dragon1;
pub mod facing;
pub mod hit_data;
pub mod hit_triggers;
pub mod movement;
pub mod player;
pub mod skill;
pub mod skill_activation;
pub mod strike;
