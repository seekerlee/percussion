//! 玩家动画状态机 —— 把 `MoveVelocity` / 按键映射成
//! [`PlayerAction`]，并把当前帧 index / 朝向写到 sprite 子实体的
//! [`TextureAtlas`] 与 quad 的 [`Transform`]。
//!
//! # 架构位置
//!
//! 视觉表达层，**不**做物理决策。读取：[`MoveVelocity`]、键盘；
//! 写入：本 unit 自己的 [`PlayerAnimationState`] 与共享的
//! [`Facing`] component，以及它 sprite 子实体的 [`Sprite`]
//! （`texture_atlas.index`）和 [`Transform`]（`scale.x` 镜像）。
//! 不读写 unit 本体的 [`Transform`] / 物理状态 / 战斗状态。
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
use crate::unit::facing::Facing;
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

/// 根据物理速度 + 按键决定当前帧应当处于哪个 [`PlayerAction`]，以
/// 及朝哪个方向。
///
/// 这是动画的"逻辑层"：只读输入 + 物理状态，只写
/// [`PlayerAnimationState`] / [`Facing`]。
///
/// # 朝向规则
///
/// `vel.0.x > MOVE_EPSILON` → 朝右；`< -MOVE_EPSILON` → 朝左；介于两
/// 者之间保持上一次朝向。这样纯 Z 轴移动 / 静止 不会引起朝
/// 向抽动，也不会被微小浮点噪声触发翻转。
fn decide_player_action(
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&MoveVelocity, &mut PlayerAnimationState, &mut Facing), With<Player>>,
) {
    for (vel, mut state, mut facing) in &mut query {
        // 朝向 —— 与动作决策独立。攻击 / Idle 过程中如果被推 / 被
        // 击退使得 x 速度有明显分量，朝向也会跟着变 —— 是否“被
        // 推动朝向”以后要不要变为“只看主动输入”，等受击玩法落地再调。
        let desired_facing = if vel.0.x > MOVE_EPSILON {
            Facing::Right
        } else if vel.0.x < -MOVE_EPSILON {
            Facing::Left
        } else {
            *facing
        };
        if *facing != desired_facing {
            *facing = desired_facing;
        }

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

/// 推进 `elapsed` 计时器、计算当前帧 index、并把 `index` + sprite quad
/// 的水平镜像同步到 sprite 子实体。
///
/// 这是动画的"渲染层"：只读 [`PlayerAnimationState`] / [`Facing`]，
/// 写到 sprite 子上的 [`Sprite::texture_atlas`] 的 `index` 和该子的
/// [`Transform::scale`] 的 x 分量。
///
/// # 为什么用 `scale.x = -1` 而不是 `Sprite::flip_x`
///
/// `bevy_sprite3d` 把 `Sprite::flip_x` 走到 `StandardMaterial::flip()` —— 这
/// 个 flip 在着色阶段把整张贴图的 U 镜像（`u' = 1 - u`），而我们当前 mesh
/// 的 UV 已经被 sprite3d 烤死指向"sheet 上某一帧"。U 镜像后会跑去 sheet
/// 镜像位置的那一帧（比如 run 帧 4 翻成 attack/jump 区），表现为"往左
/// 走时变成攻击 / 跳跃帧"。
///
/// 解决：翻 **mesh quad 本身**而不是翻 UV。给子 entity 设
/// `Transform.scale.x = -1`，绕 pivot 镜像 quad 自身，UV 不动 → 当前帧
/// 正确翻转。负 scale 不会被剔除，因为 sprite3d 出的 material 是
/// `cull_mode: None`、`double_sided = true`；`unlit = true` 让法线反向
/// 也不影响光照（没光照可影响）。
///
/// # 通过 [`Children`] 找 sprite 子
///
/// 玩家只有一个带 [`Sprite`] 的子（在 [`super::spawn_player`] 里 spawn
/// 的 sprite 节点）。没有缓存子 entity id —— 查 Children 几乎免费且不
/// 会出现误绑定。
///
/// # 为什么 guard 写入
///
/// `atlas.index = ...` 每次都会触发 `Changed<Sprite>`，bevy_sprite3d 的
/// `handle_texture_atlases` 随即跑一轮查 cache。值没变还写 = 每帧白跑
/// cache lookup，加个"真变了才写"的护栏几乎免费。`Transform` 同理。
fn tick_player_animation(
    time: Res<Time>,
    mut state_q: Query<(&mut PlayerAnimationState, &Facing, &Children), With<Player>>,
    mut sprite_q: Query<(&mut Sprite, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (mut state, facing, children) in &mut state_q {
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
        let desired_scale_x = match facing {
            Facing::Right => 1.0,
            Facing::Left => -1.0,
        };

        // 玩家 entity 的子里有且只有一个 Sprite（见 spawn_player）。
        for &child in children {
            if let Ok((mut sprite, mut transform)) = sprite_q.get_mut(child) {
                if let Some(atlas) = sprite.texture_atlas.as_mut()
                    && atlas.index != index
                {
                    atlas.index = index;
                }
                if transform.scale.x != desired_scale_x {
                    transform.scale.x = desired_scale_x;
                }
            }
        }
    }
}
