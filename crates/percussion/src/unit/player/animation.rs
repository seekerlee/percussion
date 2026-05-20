//! Player 动画状态机 —— 把 `MoveVelocity` / 按键映射成
//! [`PlayerAction`]，并把当前帧 index 写到 sprite 子实体的
//! [`TextureAtlas`]。
//!
//! # 架构位置
//!
//! 视觉表达层，**不**做物理决策。读取：[`MoveVelocity`]、键盘；
//! 写入：本 unit 自己的 [`PlayerAnimationState`] 以及它 sprite 子实体
//! 的 [`Sprite::texture_atlas`] index。不读写 [`Transform`] / 物理状态
//! / 战斗状态。
//!
//! # 调度
//!
//! 跑在 [`PostUpdate`] 且 `.after(PhysicsSystems::Prepare)`：
//! `unit::movement::apply_movement` 在 `PostUpdate` `.before(Prepare)`
//! 写完"projected velocity"后，我们读到的 [`MoveVelocity`] 就是这一
//! 帧**实际滑动后**的速度。撞墙按住前进键时 XZ ≈ 0，会被识别为静止
//! 而非移动 —— 符合 spec "run 是真的在移动、不是按方向键"。
//!
//! # 状态切换规则
//!
//! 每帧重新决策：
//!
//! 1. 攻击键 just_pressed → Attack（即使正在 Attack 也重置 = 连击）
//! 2. 否则若正在 Attack 且还没播完 → 继续 Attack
//! 3. 否则若 XZ 速度 > 阈值 → Run
//! 4. 否则 → Idle
//!
//! 攻击播完后是否回到 Run / Idle 由当帧的速度决定，零特殊状态。

use std::ops::Range;

use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;

use super::Player;
use crate::unit::movement::MoveVelocity;

/// 触发攻击的按键。临时绑定，等输入抽象层立起来再迁。
const ATTACK_KEY: KeyCode = KeyCode::KeyX;

/// XZ 平面速度模长大于多少米/秒算"在移动"。
///
/// 取小但非零：避免被浮点噪音 / 一两帧的击退残速误判成 run，也避
/// 免要求"绝对静止"导致松键瞬间的微小滑行不被识别为 idle。
const MOVE_EPSILON: f32 = 0.05;

/// 玩家动画动作枚举。
///
/// 顺序和取值与外部工具
/// [`crates/tools/src/bin/stitch_player_sprites.rs`] 生成的
/// `sheet.png` 一一对应 —— 改这里之前先改 stitcher、重跑、然后同步
/// 下面的 [`PlayerAction::range`]。
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlayerAction {
    #[default]
    Idle,
    Run,
    Attack,
    /// 已合进 sheet，但游戏逻辑这一版还没接 —— 见
    /// [`doc/game-design.md`](../../../../doc/game-design.md) 的跳跃章节。
    #[allow(dead_code)]
    Jump,
}

impl PlayerAction {
    /// 该动作在 sheet 上占据的 frame index 区间。
    ///
    /// 跟 stitcher 输出的 layout 写死同步：
    ///
    /// ```text
    /// idle    0..4
    /// run     4..11
    /// attack  11..16
    /// jump    16..20
    /// ```
    const fn range(self) -> Range<usize> {
        match self {
            Self::Idle => 0..4,
            Self::Run => 4..11,
            Self::Attack => 11..16,
            Self::Jump => 16..20,
        }
    }

    /// 该动作的播放速度（帧/秒）。
    ///
    /// 不同动作分别调：idle 慢呼吸感、attack 利落、run 居中。等真用
    /// 起来不舒服再改。
    const fn fps(self) -> f32 {
        match self {
            Self::Idle => 6.0,
            Self::Run => 12.0,
            Self::Attack => 15.0,
            Self::Jump => 12.0,
        }
    }

    /// 该动作是否循环播放。
    ///
    /// `false` = 一次性，播到最后一帧停住等被切走（Attack / Jump）。
    /// `true` = 永远循环（Idle / Run）。
    const fn looping(self) -> bool {
        match self {
            Self::Idle | Self::Run => true,
            Self::Attack | Self::Jump => false,
        }
    }

    /// 该动作完整播完一遍需要的时间（秒）。
    fn duration_secs(self) -> f32 {
        let len = self.range().end - self.range().start;
        len as f32 / self.fps()
    }
}

/// 挂在玩家 entity 上的动画状态。
///
/// 由 [`Player`] 的 `#[require]` 自动补齐 —— 没人需要在外部手动 insert。
///
/// `elapsed` 在每次动作切换时归零，配合 [`PlayerAction::fps`] 算出当
/// 前应该播第几帧。不存 frame index 本身，避免"action 变了但 frame 没
/// 同步重置"的双源同步问题。
#[derive(Component, Debug, Default)]
pub struct PlayerAnimationState {
    pub action: PlayerAction,
    pub elapsed: f32,
}

/// 注册动画的两个 system —— decide（决策） + tick（推进 + 写 sprite）。
pub struct PlayerAnimationPlugin;

impl Plugin for PlayerAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (decide_player_action, tick_player_animation)
                .chain()
                // 见模块文档"调度"一节：apply_movement 在 PostUpdate
                // `.before(PhysicsSystems::Prepare)`，我们站在它之后
                // 读 MoveVelocity，得到的就是 post-slide 速度。
                .after(PhysicsSystems::Prepare),
        );
    }
}

/// 根据物理速度 + 按键决定当前帧应当处于哪个 [`PlayerAction`]。
///
/// 这是动画的"逻辑层"：只读输入 + 物理状态，只写 [`PlayerAnimationState`]。
fn decide_player_action(
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&MoveVelocity, &mut PlayerAnimationState), With<Player>>,
) {
    for (vel, mut state) in &mut query {
        let attack_pressed = keys.just_pressed(ATTACK_KEY);
        let moving = vel.0.xz().length() > MOVE_EPSILON;

        let next = if attack_pressed {
            // 连击：即使正在 Attack 也算新一次攻击 —— 计时器重置后
            // 会从攻击第一帧重新播。
            PlayerAction::Attack
        } else if state.action == PlayerAction::Attack
            && state.elapsed < PlayerAction::Attack.duration_secs()
        {
            // 攻击未播完 —— 不被移动 / 静止覆盖（commit-to-swing）。
            PlayerAction::Attack
        } else if moving {
            PlayerAction::Run
        } else {
            PlayerAction::Idle
        };

        if next != state.action {
            state.action = next;
            state.elapsed = 0.0;
        }
    }
}

/// 推进 `elapsed` 计时器、计算当前帧 index、写到 sprite 子实体的
/// [`TextureAtlas`]。
///
/// 这是动画的"渲染层"：只读 [`PlayerAnimationState`]，写到 sprite
/// 上的 [`Sprite::texture_atlas`]。
///
/// 通过 [`Children`] 找到带 [`Sprite`] 的子 entity —— 玩家只有一个
/// 这样的子（在 [`super::spawn_player`] 里 spawn 的 sprite 节点）。
/// 没有缓存子 entity id，因为查 Children 几乎免费且不会出现误绑定。
fn tick_player_animation(
    time: Res<Time>,
    mut state_q: Query<(&mut PlayerAnimationState, &Children), With<Player>>,
    mut sprite_q: Query<&mut Sprite>,
) {
    let dt = time.delta_secs();
    for (mut state, children) in &mut state_q {
        state.elapsed += dt;
        let action = state.action;
        let range = action.range();
        let len = range.end - range.start;

        // 当前动作内的帧偏移。一次性动作（Attack / Jump）停在最后
        // 一帧，循环动作（Idle / Run）取模回到开头。
        let raw = (state.elapsed * action.fps()) as usize;
        let frame_offset = if action.looping() {
            raw % len
        } else {
            raw.min(len - 1)
        };
        let index = range.start + frame_offset;

        // 玩家 entity 的子里有且只有一个 Sprite（见 spawn_player）。
        // 这里写一次 atlas.index，bevy_sprite3d 的 handle_texture_atlases
        // 系统会下一次 PostUpdate 把对应的预烤 mesh 换上去。
        for &child in children {
            if let Ok(mut sprite) = sprite_q.get_mut(child)
                && let Some(atlas) = sprite.texture_atlas.as_mut()
            {
                atlas.index = index;
            }
        }
    }
}
