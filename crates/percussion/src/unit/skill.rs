//! Skill subsystem (skeleton) for one-shot abilities.
//!
//! This file is intentionally standalone for now:
//! - It does NOT modify existing plugin wiring.
//! - It does NOT depend on a hitbox module yet.
//! - It does NOT include channeling logic.
//!
//! Once approved, this plugin can be wired from existing modules.

use std::collections::HashMap;

use bevy::prelude::*;

// ============================================================================
// 静态数据 —— 模板（template）：spawn 时拷给 caster 作为初始值
// ============================================================================
//
// 设计要点：**全局 const 只是模板**。每个 caster 通过 [`SkillBook`] 拥有
// 自己**已实例化**的 [`SkillDefinition`]。换武器 / 上 buff / 升级招式
// 都直接修改 caster 的 SkillBook，不动模板。
//
// 这样：
// - 桥接 system 只需查 SkillBook 一处，不必 join 多个组件聚合数值
// - 不同 caster 拿不同武器 → 同一 SkillId 数值自然不同
// - 模板代码常量保留 "默认手感" 的可读性

/// Stable id for a skill definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillId {
    /// Basic one-shot melee slash.
    BasicMeleeSlash,
}

/// Tuning data for one skill.
///
/// 模板（const）和 caster 实例（[`SkillBook`] 内的 value）共用同一形状。
#[derive(Debug, Clone, Copy)]
pub struct SkillDefinition {
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
#[derive(Debug, Clone, Copy)]
pub enum SkillEffectKind {
    /// 一块**贴在 caster 身前**的长方体判定盒，跟着 [`Facing`] 转向。
    ///
    /// 几何约定（俯视图，caster 朝 +X 时）：
    ///
    /// ```text
    ///       +Z ↑
    ///          │             ┌──────────────┐ ← center.z + swing/2
    ///          │             │              │
    ///   ── ● ──┼─────────────┤   MeleeBox   ├──→ +X (facing)
    ///      P   │             │      ●       │
    ///          │             └──────────────┘ ← center.z - swing/2
    ///          │             ↑      ↑       ↑
    ///          │ ←─ off.x ──→│   center      │
    ///          │             ←──── reach ───→
    /// ```
    ///
    /// Y 方向的 box 中心默认贴 caster 中心；想做"扫腿" / "高位斩"再加 `offset_y`。
    MeleeBox {
        /// 沿 facing 方向的全长 —— **攻击够多远**（剑的长度 / 体术伸臂）。
        reach: f32,
        /// 垂直 facing 方向的全长 —— **横扫多宽**（横扫 vs 直刺）。
        swing: f32,
        /// Y 方向全长 —— **罩多高**（罩住整个人 vs 扫腿低位）。
        height: f32,
        /// caster 中心 → box 中心 的位移，**caster 平面内**。
        ///
        /// - `x`：沿 facing（正 = 朝前）。`x == reach/2` 时 box 近边贴 caster 体表。
        /// - `y`：垂直 facing（正 = facing 左手侧）。绝大多数招式 = 0，
        ///   非零用于"侧击" / 不对称挥砍。
        ///
        /// 用 `Vec2` 而不是 `Vec3`：Y 方向偏移目前没用上，等真做"扫腿"再补。
        offset: Vec2,
        /// 命中后果（伤害 / buff / 击退 …）的**声明性描述**。
        ///
        /// 见 [`HitSpec`] —— 桥接 system 会把它翻译成 spawn 出来的 hitbox
        /// entity 上的一组 `OnHit-*` 组件，每种组件由独立的 handler system
        /// 处理。这样：
        /// - 简单效果（damage）= 加字段
        /// - 复杂效果（命中给自己 buff、命中爆炸）= 加字段 + 1 component + 1 system，
        ///   彼此互不污染
        on_hit: HitSpec,
    },
}

/// 命中一次的全部后果，声明式。
///
/// 桥接层读取这里的字段，按需在 spawn 的 hitbox entity 上挂对应 `OnHit-*`
/// 组件；handler system 监听 hitbox 与 hurtbox 的碰撞事件按组件分别处理。
///
/// 现在只有 `damage`，未来加字段（all `Option<...>` 或非 `Option` 默认值）：
/// ```ignore
/// pub struct HitSpec {
///     pub damage: f32,
///     pub self_buff: Option<BuffId>,        // 命中给 caster 加 buff
///     pub target_debuff: Option<DebuffId>,  // 命中给 victim 加 debuff
///     pub knockback: Option<KnockbackSpec>,
///     pub lifesteal_ratio: f32,             // 0.0 = 无
/// }
/// ```
/// 加新字段时**只动这里 + 桥接 + 1 个 handler system**，skill 状态机不受影响。
#[derive(Debug, Clone, Copy)]
pub struct HitSpec {
    /// 一次命中造成的伤害（已是最终值 —— 已被武器 / buff 调过）。
    pub damage: f32,
}

const BASIC_MELEE_SLASH_TEMPLATE: SkillDefinition = SkillDefinition {
    cooldown: 0.45,
    windup: 0.10,
    active: 0.05,
    recovery: 0.15,
    effect: SkillEffectKind::MeleeBox {
        reach: 1.4,
        swing: 1.2,
        // 1.8 = 默认站立 unit body 高度，整个人罩住。等做"扫腿" / "高位斩"
        // 这种 Y 上有差异的招式时再调小 + 配合 box 中心 Y 偏移（暂未实现）。
        height: 1.8,
        // x = reach/2 + 0.0：box 近边正好贴 caster 体表。
        offset: Vec2::new(0.7, 0.0),
        on_hit: HitSpec { damage: 12.0 },
    },
};

/// 取一个技能的默认模板（spawn 时拷一份进 [`SkillBook`]）。
fn template(skill_id: SkillId) -> &'static SkillDefinition {
    match skill_id {
        SkillId::BasicMeleeSlash => &BASIC_MELEE_SLASH_TEMPLATE,
    }
}

// ============================================================================
// 组件 —— 挂在 unit entity 上的运行时状态
// ============================================================================

/// Per-caster **instantiated** skill definitions.
///
/// 同时回答两件事：
/// - "这个 unit 会哪些技能"（key 存在性）
/// - "这个 unit 的这招长什么样"（value —— 已被武器 / buff / 升级修改过的实例）
///
/// **内部用 `Vec` 不是 `HashMap`**：一个 unit 通常 ≤ 10 个技能，线性扫描
/// 比哈希更快，cache 也友好，调试时按"学习顺序"保留也方便。
///
/// 字段不公开 —— 外部只能通过下面的方法操作。这样：
/// - 将来"学会新技能"想挂副作用（初始化 cooldown、发 learned 消息）只改一处
/// - 武器换装统一走 [`SkillBook::get_mut`]，不会有人绕过去手动塞数据
#[derive(Component, Debug, Default, Clone)]
pub struct SkillBook {
    defs: Vec<(SkillId, SkillDefinition)>,
}

impl SkillBook {
    /// 用一组初始技能造一个 `SkillBook`，每个 skill 拷一份默认 [`template`]。
    pub fn new(initial: impl IntoIterator<Item = SkillId>) -> Self {
        let defs = initial.into_iter().map(|id| (id, *template(id))).collect();
        Self { defs }
    }

    /// 该 unit 是否拥有这个技能。
    pub fn has(&self, id: SkillId) -> bool {
        self.defs.iter().any(|(i, _)| *i == id)
    }

    /// 读这个 unit 当前的这招数据（含武器 / buff 后的最终值）。
    pub fn get(&self, id: SkillId) -> Option<&SkillDefinition> {
        self.defs.iter().find(|(i, _)| *i == id).map(|(_, d)| d)
    }

    /// 改这个 unit 的这招数据（换武器 / 上 buff 走这里）。
    pub fn get_mut(&mut self, id: SkillId) -> Option<&mut SkillDefinition> {
        self.defs.iter_mut().find(|(i, _)| *i == id).map(|(_, d)| d)
    }
}

/// Per-entity cooldown table.
#[derive(Component, Debug, Default, Clone)]
pub struct SkillCooldowns {
    pub remaining: HashMap<SkillId, f32>,
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
    pub skill_id: SkillId,
    pub phase: SkillPhase,
    pub phase_elapsed: f32,
}

// ============================================================================
// 消息 —— 跨模块通信
// ============================================================================

/// Request to start casting a specific skill on an entity.
///
/// 直接带 `SkillId` —— 调用方（玩家输入层 / AI / 调试控制台）自己
/// 决定要发哪个技能。没有 "slot" 这一层间接性，等真要做装备 / 按键
/// 绑定 / UI 槽位时再在外面包一层。
#[derive(Message, Debug, Clone, Copy)]
pub struct CastSkillRequest {
    pub caster: Entity,
    pub skill_id: SkillId,
}

/// Fired once when a cast enters Active phase.
///
/// A future hitbox/projectile module can subscribe to this and spawn effects.
#[derive(Message, Debug, Clone, Copy)]
pub struct SkillActivatedMessage {
    pub caster: Entity,
    pub skill_id: SkillId,
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
                    tick_skill_cooldowns,
                    try_start_requested_casts,
                    tick_active_casts,
                )
                    .chain(),
            );
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

        // 必须"拥有"这个技能才能放 —— 防止 AI / 调试控制台请求一个
        // unit 不会的技能。同时 def 也从这里取（caster 的实例值，不是模板）。
        let Some(def) = book.get(req.skill_id) else {
            continue;
        };

        let left = cooldowns
            .remaining
            .get(&req.skill_id)
            .copied()
            .unwrap_or(0.0);
        if left > 0.0 {
            continue;
        }

        cooldowns.remaining.insert(req.skill_id, def.cooldown);
        commands.entity(req.caster).insert(SkillCast {
            skill_id: req.skill_id,
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
        // 取 caster 实例化后的 def —— 若中途技能被移除（理论上不该发生）就清理。
        let Some(def) = book.get(cast.skill_id).copied() else {
            commands.entity(caster).remove::<SkillCast>();
            continue;
        };

        cast.phase_elapsed += dt;

        loop {
            let phase_duration = phase_duration(&def, cast.phase);
            if cast.phase_elapsed < phase_duration {
                break;
            }

            cast.phase_elapsed -= phase_duration;
            match cast.phase {
                SkillPhase::Windup => {
                    cast.phase = SkillPhase::Active;
                    activated.write(SkillActivatedMessage {
                        caster,
                        skill_id: cast.skill_id,
                        effect: def.effect,
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

fn phase_duration(def: &SkillDefinition, phase: SkillPhase) -> f32 {
    match phase {
        SkillPhase::Windup => def.windup,
        SkillPhase::Active => def.active,
        SkillPhase::Recovery => def.recovery,
    }
}
