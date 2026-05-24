//! 玩家动画状态机 —— 把当前 [`SkillCast`] / [`MoveVelocity`] 映射成
//! [`PlayerAction`]，并把当前帧 index / 朝向写到 sprite 子实体的
//! [`TextureAtlas`] 与 quad 的 [`Transform`]。
//!
//! # 架构位置
//!
//! 视觉表达层，**不**做物理决策、**不**做战斗决策。读取：[`MoveVelocity`]、
//! [`SkillCast`]、[`SkillBook`]；写入：本 unit 自己的
//! [`PlayerAnimationState`] 与共享的 [`Facing`] component，以及它 sprite
//! 子实体的 [`Sprite`]（`texture_atlas.index`）和 [`Transform`]
//! （`scale.x` 镜像）。不读写 unit 本体的 [`Transform`] / 物理状态 /
//! 战斗状态。
//!
//! # 动画完全 follow SkillCast（核心哲学）
//!
//! 攻击动画是 [`SkillCast`] 的**派生表达**，不是按键的直接产物：
//! `J` 键发 [`CastSkillRequest`](crate::unit::skill::CastSkillRequest)
//! → `try_start_requested_casts` 通过 → caster 上挂 [`SkillCast`] →
//! 本模块**看到 SkillCast 才播攻击动画**。键盘与本模块解耦。
//!
//! 这样设计的两个好处：
//!
//! 1. **同一机制可复用到 AI / 其他 unit**：dragon1 之类的 unit 只要 AI 发
//!    `CastSkillRequest`，照搬这套 "看 SkillCast 决定动作" 的模式即可，
//!    无需重新发明输入处理。
//! 2. **动画与 cast 严格同步**：动画总长 = `cast.windup + active + recovery`，
//!    且采用**三段线性划帧**：[`PlayerAction::active_frames`] 声明哪几帧是伤
//!    害帧 —— 它们精确播放在 [`SkillCast`] 的 Active 阶段里，前 / 后 两段
//!    分别填满 windup / recovery。策划调 [`Skill`] 时间数值 / 画师改伤害帧
//!    区间，两者互不踩脚、自动重新对齐。
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
//! 每帧重新决策（顶层优先级 → 末位兜底）：
//!
//! 1. 当前帧挂着 [`SkillCast`] → 按 `cast.kind` 选攻击动作；
//!    commit-to-swing 自动来自 `SkillCast` 的存在性，不需要单独兜底分支
//! 2. 否则若 XZ 速度 > 阈值 → Run
//! 3. 否则 → Idle

use std::ops::Range;

use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;

use super::Player;
use crate::unit::facing::Facing;
use crate::unit::movement::MoveVelocity;
use crate::unit::skill::{SkillBook, SkillCast, SkillKind};

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

    /// **active 帧子区间** —— 该动作中「系统效应真正生效」的帧
    /// 区间（sheet-absolute、半开区间，跟 [`range`](Self::range) 同坐标）。
    ///
    /// 借的是格斗游戏 / 动画术语里的 *active frames*：一个有阶段的动作通常
    /// 分 pre / active / post 三段，active 是「这一招在做它该做的事」的那几
    /// 帧。具体「做什么」由动作本身决定：
    ///
    /// | 动作 | active 帧的语义 | 对齐的逻辑窗 |
    /// |---|---|---|
    /// | Attack | strike 存在、判定能命中 | `Skill::active` |
    /// | Jump | 真正腾空、重力起作用 | `Jump::airborne`（未接） |
    /// | Idle / Run | —— 没有内部阶段 | `None` |
    ///
    /// `None` 不是「占位」—— 它表示「这个动作没有三段结构」。Idle / Run
    /// 是持续状态，本来就不分阶段；Jump 暂为 `None` 是因为 Jump 系统还
    /// 没接进来，等接入时填上腾空子区间。
    ///
    /// # 例：Attack 默认 13..15
    ///
    /// 整个攻击动作是 sheet 帧 11..16（5 帧）：
    ///
    /// ```text
    /// frame:   11      12      13      14      15
    ///        ├─────┼──────┼──────┼──────┼─────┤
    /// phase:   pre   pre  │   active(13..15)   │ post
    ///          两帧抬手      两帧接触、判定能命中   一帧收招
    /// ```
    ///
    /// 只要「中间一帧接触」→ `Some(13..14)`；「中间三帧都算接触」→ `Some(12..15)`。
    const fn active_frames(self) -> Option<Range<usize>> {
        match self {
            Self::Attack => Some(13..15),
            // Jump 接入时改成 Some(腾空子区间)。Idle / Run 永远 None。
            Self::Idle | Self::Run | Self::Jump => None,
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

/// 根据物理速度 + 当前 [`SkillCast`] 决定当前帧应当处于哪个
/// [`PlayerAction`]，以及朝哪个方向。
///
/// 这是动画的"逻辑层"：只读物理 / 战斗派生状态，只写
/// [`PlayerAnimationState`] / [`Facing`]。
///
/// # 朝向规则
///
/// `vel.0.x > MOVE_EPSILON` → 朝右；`< -MOVE_EPSILON` → 朝左；介于两
/// 者之间保持上一次朝向。这样纯 Z 轴移动 / 静止 不会引起朝
/// 向抽动，也不会被微小浮点噪声触发翻转。
///
/// # 为什么不再读键盘
///
/// 攻击动画的来源是 [`SkillCast`] 而不是 X 键 —— 按键先走
/// [`CastSkillRequest`](crate::unit::skill::CastSkillRequest) 这条战斗
/// 链，本模块只看 cast 是否存在。dragon1 之类的 AI unit 同样适用，
/// 不需要重写决策逻辑。
fn decide_player_action(
    mut query: Query<
        (
            &MoveVelocity,
            Option<&SkillCast>,
            &mut PlayerAnimationState,
            &mut Facing,
        ),
        With<Player>,
    >,
) {
    for (vel, cast, mut state, mut facing) in &mut query {
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

        let moving = vel.0.xz().length() > MOVE_EPSILON;

        // 1) 有 SkillCast 就播对应攻击动作（commit-to-swing 自动从
        //    SkillCast 的生命周期来 —— cast 还在就持续 Attack，cast
        //    被 tick_active_casts 移除当帧立刻回到 Run / Idle）。
        // 2) match 是 exhaustive 的：新增 SkillKind 时编译器逼着补 arm。
        let next = if let Some(cast) = cast {
            match cast.kind {
                SkillKind::BasicMeleeSlash => PlayerAction::Attack,
            }
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
///
/// # 攻击动画的三段线性划帧（与 Skill phase 对齐）
///
/// Attack 帧 fps **不**用 [`PlayerAction::fps`]。动画总长跟着 `Skill` 的
/// `windup + active + recovery` 走，但画帧不是“敦帧均匀铺”，而是按
/// [`PlayerAction::active_frames`] 声明的伤害帧区间划成三段：
///
/// ```text
/// elapsed:    0 ────────────── windup ───────── +active ───────── +recovery
/// sprite frames: [pre 抬手]               [active 接触]            [post 收招]
/// ```
///
/// pre 段在 `skill.windup` 秒内线性播完 `active_frames.start - range.start` 帧；
/// active 段在 `skill.active` 秒内线性播完伤害帧区间；post 段在 `skill.recovery`
/// 秒内线性播完剩下的帧。三段各自的局部 fps **独立**，这才能保证“active
/// 子区间恰好播于 skill 的 Active 阶段里”——于是 strike spawn (靠 Skill phase 发
/// SkillActivatedMessage 触发) 与画面上接触完美对齐。
///
/// `SkillCast` 不在时（Idle / Run / Jump）按各自的常量 fps 推进。
#[allow(clippy::type_complexity)]
fn tick_player_animation(
    time: Res<Time>,
    mut state_q: Query<
        (
            &mut PlayerAnimationState,
            &Facing,
            &Children,
            Option<&SkillCast>,
            &SkillBook,
        ),
        With<Player>,
    >,
    mut sprite_q: Query<(&mut Sprite, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (mut state, facing, children, cast, book) in &mut state_q {
        state.elapsed += dt;
        let action = state.action;
        let range = action.range();
        let len = range.end - range.start;

        // 当前动作内的帧偏移：
        // - Attack + 有 SkillCast + 声明了伤害帧区间 → 三段划帧（跳过到下面
        //   的 `paced_frame_offset` 中理）
        // - 否则一次性动作（Jump）：按常量 fps 推进、停在最后一帧
        // - 否则循环动作（Idle / Run）：按常量 fps 推进、取模回到开头
        let frame_offset = if action == PlayerAction::Attack
            && let Some(cast) = cast
            && let Some(skill) = book.get(cast.kind)
            && let Some(active_local) = action.active_frames()
        {
            // Attack 的三段时长来自 Skill。将来 Jump / 其他有阶段的动作
            // 可以复用 `paced_frame_offset`，只是时长源不同。
            paced_frame_offset(
                state.elapsed,
                range.clone(),
                active_local,
                skill.windup,
                skill.active,
                skill.recovery,
            )
        } else {
            let raw = (state.elapsed * action.fps()) as usize;
            if action.looping() {
                raw % len
            } else {
                raw.min(len - 1)
            }
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

/// **纯函数**：把三段时长（pre / active / post 各自的秒数）映射成
/// 当前帧在 `range` 上的偏移（0..len），让 `active` 子区间恰好播于
/// `[pre_secs, pre_secs + active_secs)` 的时间窗里。
///
/// 这是个**与调度源解耦**的工具：调用方负责把时长喂进来 —— Attack 从
/// [`Skill`] 取（windup / active / recovery），Jump 将来从自己的状态取
/// （prep / airborne / landing），数学一样。
///
/// # 三段设计
///
/// 设 `range = 11..16`（总长 5）、`active = 13..15`：
/// - **pre**    ： `11..13`（长 2）, 在 `pre_secs` 内线性播完
/// - **active** ： `13..15`（长 2）, 在 `active_secs` 内线性播完
/// - **post**   ： `15..16`（长 1）, 在 `post_secs` 内线性播完
///
/// 每段独立线性 = 「active 子区间出现在 `[pre_secs, pre_secs + active_secs)`」
/// 这条保证不依赖任何外部对齐机制。
///
/// # 边界 & 退化
///
/// - 某一段秒数 ≤ 0：跳过该段，帧位留在本段起点。
/// - `elapsed >= total`：clamp 到最后一帧（`len - 1`）。
/// - `active` 必须是 `range` 的非空子区间 —— `debug_assert!` 检查。
fn paced_frame_offset(
    elapsed: f32,
    range: Range<usize>,
    active: Range<usize>,
    pre_secs: f32,
    active_secs: f32,
    post_secs: f32,
) -> usize {
    debug_assert!(
        active.start >= range.start && active.end <= range.end && active.start < active.end,
        "active {:?} must be a non-empty sub-range of range {:?}",
        active,
        range,
    );

    let len = range.end - range.start;
    let pre_len = active.start - range.start;
    let act_len = active.end - active.start;
    // post_len 后面不直接用 —— 逆向划帧时靠 `len - pre_len - act_len` 隐式表达。

    /// 在 "本段合计 `seg_secs` 秒内播 `seg_len` 帧" 的约束下，算当前子偏移。
    fn linear(local_t: f32, seg_secs: f32, seg_len: usize) -> usize {
        if seg_secs <= 0.0 || seg_len == 0 {
            return 0;
        }
        ((local_t / seg_secs) * seg_len as f32) as usize
    }

    let raw = if elapsed < pre_secs {
        // pre 段。pre_len = 0 时 raw = 0，帧就是 range.start（= active.start）——
        // 表示画师未画 anticipation 帧、windup 期间顯示挥击首帧作为 poised 姿。
        linear(elapsed, pre_secs, pre_len)
    } else if elapsed < pre_secs + active_secs {
        // active 段
        pre_len + linear(elapsed - pre_secs, active_secs, act_len)
    } else {
        // post 段
        let post_len = len - pre_len - act_len;
        let local = elapsed - pre_secs - active_secs;
        pre_len + act_len + linear(local, post_secs, post_len)
    };
    raw.min(len - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// elapsed = 0 应处于首帧（pre 段起点）。
    #[test]
    fn frame_zero_at_start() {
        assert_eq!(paced_frame_offset(0.0, 11..16, 12..14, 0.10, 0.05, 0.15), 0);
    }

    /// elapsed 刚达 pre 边界 → 进入 active 首帧（pre_len = 1，偏移 1）。
    #[test]
    fn enters_active_at_pre_boundary() {
        assert_eq!(
            paced_frame_offset(0.10, 11..16, 12..14, 0.10, 0.05, 0.15),
            1,
        );
    }

    /// active 阶段后半段 → 偏移到 active 区间的第二帧（pre_len + 1）。
    ///
    /// 不取正中点（t = 0.125）是因为 `(0.025 / 0.05) * 2` 在 f32 下不严格 = 1.0，
    /// 取 75% 处避开浮点抖动。
    #[test]
    fn second_half_of_active_hits_second_active_frame() {
        // t = 0.1375（active 75% 处），local_t = 0.0375，0.0375/0.05*2 = 1.5 → +1
        // 到 pre_len + 1 = 2
        assert_eq!(
            paced_frame_offset(0.1375, 11..16, 12..14, 0.10, 0.05, 0.15),
            2,
        );
    }

    /// elapsed 刚达 pre+active 边界 → 进入 post 首帧（pre_len + act_len = 3）。
    #[test]
    fn enters_post_at_active_boundary() {
        assert_eq!(
            paced_frame_offset(0.15, 11..16, 12..14, 0.10, 0.05, 0.15),
            3,
        );
    }

    /// elapsed 超过 total → clamp 到最后一帧。
    #[test]
    fn clamps_after_total() {
        // total = 0.30，超出后应停在最后一帧（len - 1 = 4）。
        assert_eq!(
            paced_frame_offset(0.50, 11..16, 12..14, 0.10, 0.05, 0.15),
            4,
        );
    }

    /// 未声明抬手帧（pre_len = 0）：elapsed < pre_secs 依然在帧 0 作为 poised 姿。
    #[test]
    fn no_pre_frames_keeps_first_frame_during_pre() {
        // active 起点 = range 起点 → pre_len = 0
        assert_eq!(
            paced_frame_offset(0.05, 11..16, 11..14, 0.10, 0.05, 0.15),
            0,
        );
        // 进入 active 边界 → 仍是 0（pre_len = 0，linear 起点 = 0）
        assert_eq!(
            paced_frame_offset(0.10, 11..16, 11..14, 0.10, 0.05, 0.15),
            0,
        );
    }

    /// pre_secs = 0（瞬发招）：t = 0 直接落在 active 首帧。
    #[test]
    fn zero_pre_starts_in_active() {
        assert_eq!(paced_frame_offset(0.0, 11..16, 12..14, 0.0, 0.10, 0.20), 1,);
    }
}
