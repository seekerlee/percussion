//! Skill subsystem (skeleton) for one-shot abilities.
//!
//! This file is intentionally standalone for now:
//! - It does NOT modify existing plugin wiring.
//! - It does NOT include channeling logic.
//!
//! Once approved, this plugin can be wired from existing modules.
//!
//! # 命名约定（"Skill" 这个词指什么）
//!
//! 一致性规则：**"Skill" 在代码里只指"一招的运行时数据实例"**，即
//! [`Skill`] 这个 struct（含 kind + 当前数值）。**抽象的"哪一招"**用
//! [`SkillKind`] 表达 —— 仅作为身份标签（enum），不带数值。
//!
//! 所以：
//! - `Skill` = 一招（含 kind 字段 + cooldown / 数值 / effect）
//! - `SkillKind` = 招的种类身份（`BasicMeleeSlash` 这种）
//! - `SkillKindSet` = caster 学会了哪几种招（intent 层 —— 只装 kind）
//! - `SkillBook` = caster 当前帧的 [`Skill`] 实例集合（cache 层 —— 装完整 Skill）

use std::collections::HashMap;

use bevy::prelude::*;

use super::Strength;
use super::hit_data::{DamageModifier, HitSpec};

// ============================================================================
// 数据流总览：intent → cache，由 recompute 系统单向推导
// ============================================================================
//
// ```text
//   SkillKindSet (intent) ─┐
//   Strength (source)     ─┼──► recompute_skill_book ──► SkillBook (cache)
//   <future: Buffs>       ─┤           （Update 头部）        ▲
//   <future: Equipped>    ─┘                                  │
//                                                             │
//                                cast / strike 系统只读 ──────┘
// ```
//
// **intent**（[`SkillKindSet`]）= "这个 caster 会哪些招"，玩法层写。
// **source** = caster 身上一切影响招式数值的组件（[`Strength`] / 未来的
//   buff / equipment）。
// **cache**（[`SkillBook`]）= 当前帧最终的 [`Skill`] 实例，由
//   [`recompute_skill_book`] 在 intent / source 变化时按 [`template`] +
//   一连串 `apply_xxx` 重算。
//
// 哲学：
// - cast / strike 子系统**只读 SkillBook**，不再去 join 其他 caster 组件
//   聚合数值。"力量加多少伤害"这种数值知识集中在 recompute 里一处。
// - 玩法层只动 intent / source（学会招式、上 buff、换装备）；不直接写
//   SkillBook。SkillBook 由 recompute 自动跟上。
// - 加新 source？给 `recompute_skill_book` 的 query 加一项 + 加一个
//   `apply_xxx` 即可，其他模块零改动。
//
// 顺序 / 加法 vs 乘法 / 优先级等"数值具体怎么折算"问题留给真正实现
// 每条 modifier 时再定 —— 现在打好骨架就够。

/// 招式种类身份（kind）—— 仅作为标签，不带数值。
///
/// "我会哪几招"用 [`SkillKindSet`] 装一组 `SkillKind`；
/// "这招当前数值多少"由 [`recompute_skill_book`] 折算成 [`Skill`] 后从
/// [`SkillBook`] 读。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillKind {
    /// Basic one-shot melee slash.
    BasicMeleeSlash,
}

/// 一招的运行时数据 —— 含身份（[`kind`](Self::kind)）+ 当前数值。
///
/// 模板（[`template`] fn 返回值）和 caster 实例（[`SkillBook`] 内的元素）
/// 共用同一形状。
///
/// **不再是 `Copy`**：内含 [`HitSpec`] 持有 `Vec<DamageModifier>` /
/// `Vec<HitTrigger>`，自然不能 Copy。所有需要传递的地方走 `Clone`，
/// 一个 caster 通常 ≤ 10 招，clone 成本可忽略。
#[derive(Debug, Clone)]
pub struct Skill {
    /// 这招是哪一种 —— 让 `Skill` 本身自识别，无需外层用 `(kind, data)` 元组。
    pub kind: SkillKind,
    pub cooldown: f32,
    pub windup: f32,
    pub active: f32,
    pub recovery: f32,
    pub effect: SkillEffectKind,
}

/// Effect payload emitted on activation.
///
/// 字段命名走 **caster-relative 语义**而不是世界坐标 `width / depth / height`，
/// 因为命中盒会跟着 [`Facing`](super::facing::Facing) 旋转 —— 用世界轴命名
/// 在 `Facing::Left` 时所有"沿 X 轴"的字段语义都翻一遍，让人糊涂。
#[derive(Debug, Clone)]
pub enum SkillEffectKind {
    /// **沿 [`Facing`] 朝前的一段直线** —— "一刀能挥多远"。
    ///
    /// 几何约定（俯视图，caster 朝 +X 时）：caster 中心 P 出发，沿
    /// +X 走 `offset.x` 抵达"判定中心"，再向前 `reach/2` 是攻击边界。
    ///
    /// ```text
    ///       +Z ↑
    ///          │
    ///          │
    ///   ── ● ──┼─────── center ──────────●  ← 攻击最远点（offset.x + reach/2）
    ///      P   │       (offset.x)
    ///          │
    /// ```
    ///
    /// 桥接 system（[`super::skill_strike`]）把它翻译成
    /// [`AttackEffect::MeleeReach`](super::strike::AttackEffect::MeleeReach)
    /// 的圆形点-距判定：在以 `(offset.x + reach/2)` 为半径的圆里找最
    /// 近目标。当前实现**忽略 reach 后半段**视觉宽度差异（横扫 vs 直
    /// 刺手感等价）—— 单段距离判定够首版。
    ///
    /// **TODO（扇形 / 锥形扩展）**：要做"横扫 vs 直刺"或"扇形 AoE"，
    /// 加新 variant（如 `MeleeFan { reach, half_angle, offset, on_hit }`
    /// / `Aoe { radius, cone, on_hit }`），bridge 翻成
    /// [`AttackEffect::Aoe`](super::strike::AttackEffect::Aoe) +
    /// [`Cone`](super::strike::Cone)（已就位）。**不要**把扇形参数加
    /// 回 `MeleeReach` —— 那会把"直线"和"扇形"两种语义糊在一起。
    MeleeReach {
        /// 沿 facing 方向的全长 —— **攻击够多远**（剑的长度 / 体术伸臂）。
        reach: f32,
        /// caster 中心 → 判定中心 的位移，**caster 平面内**。
        ///
        /// - `x`：沿 facing（正 = 朝前）。`x == reach/2` 时判定近边
        ///   贴 caster 体表。
        /// - `y`：垂直 facing（正 = facing 左手侧）。绝大多数招式 = 0,
        ///   非零用于"侧击" / 不对称挥砍。当前 `MeleeReach` 桥接
        ///   只读 `offset.x`；`offset.y` 等扇形 variant 加入再启用。
        ///
        /// 用 `Vec2` 而不是 `Vec3`：俯视战斗里 Y 偏移留给"扫腿 / 跳劈"
        /// 这种 Y 区分需求出现时再补。
        offset: Vec2,
        /// 命中后果（伤害 / buff / 击退 …）的**声明性描述**。
        ///
        /// 见 [`HitSpec`] —— 桥接 system 用它构造 [`Strike`] 的
        /// `on_hit`，命中那一刻 clone 进 `CollisionMessage`，下游
        /// modifier / trigger 流水线消费。
        on_hit: HitSpec,
    },
}

/// 取一招的默认模板 —— 不带任何 caster-side 修正的"裸数值"。
///
/// [`recompute_skill_book`] 会以这个为起点，依次叠 caster 身上各 source
/// 组件（[`Strength`] / 未来的 buff / equipment），得到最终的 cached
/// [`Skill`] 塞进 [`SkillBook`]。
///
/// **由 fn 返回 owned 值而不是 `&'static const`**：[`HitSpec`] 持有 `Vec`
/// （为 modifier / trigger 流水线），Vec 不能在 const context 里构造。
/// 改成 fn 之后每次调用构造一份新值；调用频率 = 重算频率（intent / source
/// 变化时，远低于每帧），无热路径开销。
fn template(kind: SkillKind) -> Skill {
    match kind {
        SkillKind::BasicMeleeSlash => Skill {
            kind,
            cooldown: 0.45,
            windup: 0.10,
            active: 0.05,
            recovery: 0.15,
            effect: SkillEffectKind::MeleeReach {
                reach: 1.4,
                // x = reach/2 + 0.0：判定近边正好贴 caster 体表。
                offset: Vec2::new(0.7, 0.0),
                // 首版只挂裸伤害：modifiers / triggers 都空。
                // bridge 在 spawn strike 时会按 caster.Strength 等 prepend
                // [`DamageModifier::Mul`] 到 modifiers 头部 —— 模板本身
                // 不需要预声明 caster-side 修正。
                on_hit: HitSpec {
                    base_damage: 12.0,
                    modifiers: Vec::new(),
                    triggers: Vec::new(),
                },
            },
        },
    }
}

// ============================================================================
// 组件 —— 挂在 unit entity 上的运行时状态
// ============================================================================

/// Caster 的招式**意图集**：声明"这个 unit 会哪几种招"。
///
/// 这是玩法层唯一应该写的入口 —— spawn 时 `SkillKindSet::new([...])`，
/// 学会新招就 push 一个 [`SkillKind`] 进去。**不存数值** —— 数值由
/// [`recompute_skill_book`] 从 [`template`] + caster source 组件算到
/// [`SkillBook`] 里。
///
/// `#[require(SkillBook, SkillCooldowns)]`：spawn `SkillKindSet` 时 Bevy
/// 自动补这两个 derived 组件，调用方不用手动挂。第一帧 recompute 跑完后
/// SkillBook 就有内容；recompute 排在技能链头部，跟 cast 系统在同一帧。
///
/// **内部用 `Vec` 不是 `HashSet`**：≤ 10 种招，线性扫描比哈希更友好；
/// 按"学习顺序"保留也方便调试。
#[derive(Component, Debug, Default, Clone)]
#[require(SkillBook, SkillCooldowns)]
pub struct SkillKindSet {
    kinds: Vec<SkillKind>,
}

impl SkillKindSet {
    /// 用一组初始 kind 造一个 `SkillKindSet`。
    pub fn new(initial: impl IntoIterator<Item = SkillKind>) -> Self {
        Self {
            kinds: initial.into_iter().collect(),
        }
    }

    /// 该 unit 是否拥有这种招（intent 层判断 —— 跟当前数值无关）。
    pub fn has(&self, kind: SkillKind) -> bool {
        self.kinds.contains(&kind)
    }

    /// 遍历该 unit 拥有的所有招式 kind。
    pub fn kinds(&self) -> impl Iterator<Item = SkillKind> + '_ {
        self.kinds.iter().copied()
    }
}

/// Caster 当前帧的**招式实例缓存**：[`recompute_skill_book`] 算出的
/// 当前帧最终 [`Skill`] 集合。
///
/// **derived cache —— 玩法层不要直接写它**。要改 caster 的招式数值，请
/// 改 intent（[`SkillKindSet`]）或 source（[`Strength`] / buff / equipment），
/// recompute 会自动跟上。
///
/// 字段对模块外完全不可写：没有 `new` / `get_mut` / pub field，只能读。
/// recompute 系统跟本组件同模块，直接写 `book.skills`。
///
/// 命名上 "Book" 是相对 "KindSet" 的：KindSet 是招名册（学会哪几种），
/// Book 是招式秘籍（每招当前的完整 [`Skill`] 实例）。
#[derive(Component, Debug, Default, Clone)]
pub struct SkillBook {
    skills: Vec<Skill>,
}

impl SkillBook {
    /// 读这个 unit 当前的这招（含 source 折算后的最终值）。
    pub fn get(&self, kind: SkillKind) -> Option<&Skill> {
        self.skills.iter().find(|s| s.kind == kind)
    }
}

/// Per-entity cooldown table.
#[derive(Component, Debug, Default, Clone)]
pub struct SkillCooldowns {
    pub remaining: HashMap<SkillKind, f32>,
}

/// One-shot cast phases (channeling intentionally omitted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillPhase {
    Windup,
    Active,
    Recovery,
}

/// Current cast state attached to a caster entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct SkillCast {
    pub kind: SkillKind,
    pub phase: SkillPhase,
    pub phase_elapsed: f32,
}

// ============================================================================
// 消息 —— 跨模块通信
// ============================================================================

/// Request to start casting a specific skill on an entity.
///
/// 直接带 [`SkillKind`] —— 调用方（玩家输入层 / AI / 调试控制台）自己
/// 决定要发哪招。没有 "slot" 这一层间接性，等真要做装备 / 按键
/// 绑定 / UI 槽位时再在外面包一层。
#[derive(Message, Debug, Clone, Copy)]
pub struct CastSkillRequest {
    pub caster: Entity,
    pub kind: SkillKind,
}

/// Fired once when a cast enters Active phase.
///
/// [`super::skill_strike`] / [`crate::projectile`] subscribe to this and spawn
/// effects (Strike entity / projectile).
///
/// **不再是 `Copy`**：[`SkillEffectKind`] 内含 [`HitSpec`]（持有 Vec），
/// 自然不能 Copy。消费方对 `ev: &SkillActivatedMessage` 用 `match &ev.effect`
/// 模式匹配引用、需要克隆时显式 `.clone()`。
#[derive(Message, Debug, Clone)]
pub struct SkillActivatedMessage {
    pub caster: Entity,
    pub kind: SkillKind,
    pub effect: SkillEffectKind,
}

// ============================================================================
// 插件 + 系统
// ============================================================================

/// Plugin skeleton for one-shot skill casting.
pub struct SkillPlugin;

impl Plugin for SkillPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CastSkillRequest>()
            .add_message::<SkillActivatedMessage>()
            .add_systems(
                Update,
                (
                    // recompute 必须在 cast / activate 系统之前 —— 这样
                    // intent / source 同帧变化能立刻反映到本帧的施法 +
                    // strike spawn。skill_strike 的 bridge 也读 SkillBook
                    // 拿 active 时长，链头放 recompute 同样保险。
                    recompute_skill_book,
                    tick_skill_cooldowns,
                    try_start_requested_casts,
                    tick_active_casts,
                )
                    .chain(),
            );
    }
}

/// 把 caster 的 intent（[`SkillKindSet`]）+ source 组件折算成
/// [`SkillBook`] 缓存。
///
/// 触发条件：[`SkillKindSet`] 或任何 source 组件（[`Strength`] / 未来
/// buff / equipment）`Changed` —— Bevy 的 `Or<(Changed<...>)>` filter 自动
/// 覆盖"新加该组件"（Added 蕴含 Changed），所以**spawn 时也会触发**，无需
/// 单独的 observer。
///
/// 实现策略：**全量重算**，不增量。每个 caster ≤ 10 招，每次 recompute
/// 的工作量是 "template clone + 几条 apply_xxx"，纯 CPU 数百次浮点；
/// 100 个 unit 同帧换 buff 也是微秒级，跟增量管理的复杂度不划算。
///
/// 加新 source 的标准做法：
/// 1. 在 query 元组加 `Option<&NewSource>`
/// 2. 在 `Or<>` filter 加 `Changed<NewSource>`
/// 3. 在 `compute_skill` 里加一个 `apply_new_source` 调用
///
/// 顺序：apply_xxx 按"先放大、再修正"的直觉链接 —— 见 [`HitSpec`] 上
/// 关于 [`DamageModifier`] 顺序的约定。当前唯一一项是 [`apply_strength`]，
/// 顺序问题等真要叠多 source 时再定。
#[allow(clippy::type_complexity)]
fn recompute_skill_book(
    mut q: Query<
        (&SkillKindSet, Option<&Strength>, &mut SkillBook),
        Or<(Changed<SkillKindSet>, Changed<Strength>)>,
    >,
) {
    for (set, strength, mut book) in &mut q {
        book.skills = set
            .kinds
            .iter()
            .map(|kind| compute_skill(*kind, strength))
            .collect();
    }
}

/// 从 [`template`] 起步，依次叠所有 caster source，得到一招的最终
/// [`Skill`]。新 source 在这里加一个 `apply_xxx` 调用。
fn compute_skill(kind: SkillKind, strength: Option<&Strength>) -> Skill {
    let mut skill = template(kind);
    apply_strength(&mut skill, strength);
    // future: apply_equipment(&mut skill, equipped);
    // future: apply_buffs(&mut skill, buffs);
    skill
}

/// 把 caster [`Strength`] 烙进招的 hit 输出 modifier。
///
/// 当前只对 [`SkillEffectKind::MeleeReach`] 的 `on_hit` 起作用 —— 因为目前
/// 只有这一种 effect。加新 effect kind 时如果它也产生伤害，需要在这里
/// 加一个 match 分支把 strength 注入它的 [`HitSpec`]。
///
/// `Strength` 缺失或 = 1.0 时不加任何 modifier —— 让默认 unit 的
/// `modifiers` 链保持空、调试输出简洁。
fn apply_strength(skill: &mut Skill, strength: Option<&Strength>) {
    let s = strength.map_or(1.0, |s| s.0);
    if s == 1.0 {
        return;
    }
    match &mut skill.effect {
        SkillEffectKind::MeleeReach { on_hit, .. } => {
            // prepend：caster-side 整体倍率先 apply，让后续 modifier
            // （buff / 命中端加成）作用在已放大的结果上。
            on_hit.modifiers.insert(0, DamageModifier::Mul(s));
        }
    }
}

fn tick_skill_cooldowns(time: Res<Time>, mut q: Query<&mut SkillCooldowns>) {
    let dt = time.delta_secs();
    for mut cooldowns in &mut q {
        for value in cooldowns.remaining.values_mut() {
            *value = (*value - dt).max(0.0);
        }
    }
}

fn try_start_requested_casts(
    mut requests: MessageReader<CastSkillRequest>,
    mut q_caster: Query<(&SkillBook, &mut SkillCooldowns, Option<&SkillCast>)>,
    mut commands: Commands,
) {
    for req in requests.read() {
        let Ok((book, mut cooldowns, cast)) = q_caster.get_mut(req.caster) else {
            continue;
        };
        if cast.is_some() {
            // 已经在施法中 —— 不打断、不排队。打断走未来的 CancelSkillRequest。
            continue;
        }

        // 必须"拥有"这招才能放 —— 防止 AI / 调试控制台请求一个
        // unit 不会的招。同时 skill 也从这里取（caster 的实例值，不是模板）。
        let Some(skill) = book.get(req.kind) else {
            continue;
        };

        let left = cooldowns.remaining.get(&req.kind).copied().unwrap_or(0.0);
        if left > 0.0 {
            continue;
        }

        cooldowns.remaining.insert(req.kind, skill.cooldown);
        commands.entity(req.caster).insert(SkillCast {
            kind: req.kind,
            phase: SkillPhase::Windup,
            phase_elapsed: 0.0,
        });
    }
}

fn tick_active_casts(
    time: Res<Time>,
    mut q_cast: Query<(Entity, &SkillBook, &mut SkillCast)>,
    mut activated: MessageWriter<SkillActivatedMessage>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    for (caster, book, mut cast) in &mut q_cast {
        // 取 caster 实例化后的 skill —— 若中途技能被移除（理论上不该发生）就清理。
        // **借用不拷贝**：Skill 不再 Copy（HitSpec 持 Vec），
        // 只在真要发 message 时 clone effect。
        let Some(skill) = book.get(cast.kind) else {
            commands.entity(caster).remove::<SkillCast>();
            continue;
        };

        cast.phase_elapsed += dt;

        loop {
            let duration = phase_duration(skill, cast.phase);
            if cast.phase_elapsed < duration {
                break;
            }

            cast.phase_elapsed -= duration;
            match cast.phase {
                SkillPhase::Windup => {
                    cast.phase = SkillPhase::Active;
                    activated.write(SkillActivatedMessage {
                        caster,
                        kind: cast.kind,
                        effect: skill.effect.clone(),
                    });
                }
                SkillPhase::Active => {
                    cast.phase = SkillPhase::Recovery;
                }
                SkillPhase::Recovery => {
                    commands.entity(caster).remove::<SkillCast>();
                    break;
                }
            }
        }
    }
}

fn phase_duration(skill: &Skill, phase: SkillPhase) -> f32 {
    match phase {
        SkillPhase::Windup => skill.windup,
        SkillPhase::Active => skill.active,
        SkillPhase::Recovery => skill.recovery,
    }
}
