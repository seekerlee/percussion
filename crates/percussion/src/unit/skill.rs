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
// 静态数据 —— 不挂 entity，描述"技能本身"的定义
// ============================================================================

/// Stable id for a skill definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillId {
    /// Basic one-shot melee slash.
    BasicMeleeSlash,
}

/// Static tuning data for one skill.
#[derive(Debug, Clone, Copy)]
pub struct SkillDefinition {
    pub cooldown: f32,
    pub windup: f32,
    pub active: f32,
    pub recovery: f32,
    pub effect: SkillEffectKind,
}

/// Effect payload emitted on activation.
#[derive(Debug, Clone, Copy)]
pub enum SkillEffectKind {
    MeleeBox {
        width: f32,
        depth: f32,
        forward_offset: f32,
        damage: f32,
    },
}

const BASIC_MELEE_SLASH_DEF: SkillDefinition = SkillDefinition {
    cooldown: 0.45,
    windup: 0.10,
    active: 0.05,
    recovery: 0.15,
    effect: SkillEffectKind::MeleeBox {
        width: 1.2,
        depth: 1.4,
        forward_offset: 0.7,
        damage: 12.0,
    },
};

fn definition(skill_id: SkillId) -> &'static SkillDefinition {
    match skill_id {
        SkillId::BasicMeleeSlash => &BASIC_MELEE_SLASH_DEF,
    }
}

// ============================================================================
// 组件 —— 挂在 unit entity 上的运行时状态
// ============================================================================

/// Skills this unit knows / is allowed to cast.
///
/// 纯数据：一个 unit 拥有哪些技能。不涉及 UI 槽位 / 按键绑定 ——
/// 那些是更外层的概念（出现需求时再加 `EquippedSkillSlots` 之类）。
///
/// 用 `Vec` 而不是 `HashSet`：技能数量天然很少（个位数），线性扫描比
/// 哈希更快，也保留确定顺序便于调试。
///
/// 内部字段**不**公开 —— 外部只能通过 [`Skills::new`] 构造、`has` 查询。
/// 这样将来：
/// - 想在"加技能"时挂副作用（如初始化 cooldown、发 learned 消息）只改一处
/// - 想换内部表示（`SmallVec` / `HashSet`）只改一处
/// - 不会有人在 system 里写 `skills.0.push(...)` 绕过领域规则
#[derive(Component, Debug, Default, Clone)]
pub struct Skills(Vec<SkillId>);

impl Skills {
    /// 用一组初始技能造一个 `Skills`。
    pub fn new(skills: Vec<SkillId>) -> Self {
        Self(skills)
    }

    /// 该 unit 是否拥有这个技能。
    pub fn has(&self, id: SkillId) -> bool {
        self.0.contains(&id)
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
    mut q_caster: Query<(&Skills, &mut SkillCooldowns, Option<&SkillCast>)>,
    mut commands: Commands,
) {
    for req in requests.read() {
        let Ok((skills, mut cooldowns, cast)) = q_caster.get_mut(req.caster) else {
            continue;
        };
        if cast.is_some() {
            // 已经在施法中 —— 不打断、不排队。打断走未来的 CancelSkillRequest。
            continue;
        }

        // unit 必须"拥有"这个技能才能放 —— 防止 AI / 调试控制台请求一个
        // unit 不会的技能。
        if !skills.has(req.skill_id) {
            continue;
        }

        let def = definition(req.skill_id);
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
    mut q_cast: Query<(Entity, &mut SkillCast)>,
    mut activated: MessageWriter<SkillActivatedMessage>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    for (caster, mut cast) in &mut q_cast {
        cast.phase_elapsed += dt;

        loop {
            let phase_duration = current_phase_duration(*cast);
            if cast.phase_elapsed < phase_duration {
                break;
            }

            cast.phase_elapsed -= phase_duration;
            match cast.phase {
                SkillPhase::Windup => {
                    cast.phase = SkillPhase::Active;
                    let def = definition(cast.skill_id);
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

fn current_phase_duration(cast: SkillCast) -> f32 {
    let def = definition(cast.skill_id);
    match cast.phase {
        SkillPhase::Windup => def.windup,
        SkillPhase::Active => def.active,
        SkillPhase::Recovery => def.recovery,
    }
}
