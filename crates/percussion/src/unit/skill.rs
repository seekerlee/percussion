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

/// Stable id for a skill definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillId {
    /// Basic one-shot melee slash.
    BasicMeleeSlash,
}

/// Which slot the caller wants to cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillSlot {
    Basic,
}

/// Runtime skill slots carried by an entity.
#[derive(Component, Debug, Clone)]
pub struct Skills {
    pub basic: Option<SkillId>,
}

impl Default for Skills {
    fn default() -> Self {
        Self {
            basic: Some(SkillId::BasicMeleeSlash),
        }
    }
}

impl Skills {
    pub fn get(&self, slot: SkillSlot) -> Option<SkillId> {
        match slot {
            SkillSlot::Basic => self.basic,
        }
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

/// Request to start casting a slot on an entity.
#[derive(Message, Debug, Clone, Copy)]
pub struct CastSkillRequest {
    pub caster: Entity,
    pub slot: SkillSlot,
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
            continue;
        }

        let Some(skill_id) = skills.get(req.slot) else {
            continue;
        };
        let def = definition(skill_id);

        let left = cooldowns.remaining.get(&skill_id).copied().unwrap_or(0.0);
        if left > 0.0 {
            continue;
        }

        cooldowns.remaining.insert(skill_id, def.cooldown);
        commands.entity(req.caster).insert(SkillCast {
            skill_id,
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
