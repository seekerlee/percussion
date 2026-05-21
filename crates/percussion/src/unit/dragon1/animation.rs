//! Dragon1 动画 —— 9 帧扇翅膀循环。
//!
//! # 跟 player animation 的差异
//!
//! 没有状态机：dragon1 目前只有"飞"一个动作，永远循环。等接入 AI /
//! 攻击 / 受击之后再考虑像 [`super::super::player::animation`] 那样
//! 抽 `Dragon1Action` enum + decide 系统。今天加属于猜测性扩展，不做。
//!
//! # 调度
//!
//! 跑在 [`Update`]：不像 player 那样依赖 post-physics 的 [`MoveVelocity`]
//! \—— 单循环动画只看时间，跟物理 / 输入完全无关。

use bevy::prelude::*;

use super::Dragon1;

/// sheet 上的帧数（与 `sunny-dragon-fly.png` 的列数一致，1 行 9 帧）。
const DRAGON1_FRAME_COUNT: usize = 9;

/// 扇翅膀速度（帧/秒）。
///
/// 9 帧 / 8 fps ≈ 1.125 秒一轮，给"悬停慢扇翅膀"的视觉节奏。等真用
/// 起来快了 / 慢了再调。
const DRAGON1_FPS: f32 = 8.0;

/// 单只 dragon1 的动画推进状态。
///
/// 由 [`Dragon1`] 的 `#[require]` 自动挂上 —— 跟 [`super::super::player`]
/// 的 [`PlayerAnimationState`](super::super::player::animation::PlayerAnimationState)
/// 一样，不存 frame index 本身（避免双源同步问题），只存 `elapsed`、
/// 推进时再算出 index。
#[derive(Component, Debug, Default)]
pub struct Dragon1AnimationState {
    pub elapsed: f32,
}

/// 注册 dragon1 动画 tick system。
pub struct Dragon1AnimationPlugin;

impl Plugin for Dragon1AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, tick_dragon1_animation);
    }
}

/// 推进 `elapsed`、按 [`DRAGON1_FPS`] 算出当前帧 index、并 guard-写入
/// sprite 子实体的 [`TextureAtlas::index`]。
///
/// # 通过 [`Children`] 找 sprite 子
///
/// 跟 player 同构：dragon1 entity 只有一个带 [`Sprite`] 的子（在
/// [`super::spawn_dragon1`] 里 spawn 的 sprite 节点）。
///
/// # 为什么 guard 写入
///
/// 写 `atlas.index = ...` 会触发 `Changed<Sprite>`，bevy_sprite3d 的
/// `handle_texture_atlases` 随后跑一轮 cache lookup。值没变还写就是
/// 每帧白跑，加个"真变了才写"几乎免费。
fn tick_dragon1_animation(
    time: Res<Time>,
    mut state_q: Query<(&mut Dragon1AnimationState, &Children), With<Dragon1>>,
    mut sprite_q: Query<&mut Sprite>,
) {
    let dt = time.delta_secs();
    for (mut state, children) in &mut state_q {
        state.elapsed += dt;
        // % 1 周期防止 elapsed 长时间累计后浮点精度下降；周期 = 总播放时长。
        let cycle = DRAGON1_FRAME_COUNT as f32 / DRAGON1_FPS;
        if state.elapsed >= cycle {
            state.elapsed %= cycle;
        }
        let index = (state.elapsed * DRAGON1_FPS) as usize % DRAGON1_FRAME_COUNT;

        for &child in children {
            if let Ok(mut sprite) = sprite_q.get_mut(child)
                && let Some(atlas) = sprite.texture_atlas.as_mut()
                && atlas.index != index
            {
                atlas.index = index;
            }
        }
    }
}
