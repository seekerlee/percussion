//! Player —— 受键盘输入驱动的 [`Unit`](crate::unit::Unit)。
//!
//! # 与 Unit 的关系
//!
//! [`Player`] 是 [`Unit`] 的一种特化身份："这个 unit 被键盘驱动"。通过
//! `#[require(Unit)]` 声明，spawn `Player` 时 Bevy 自动补上 `Unit` marker。
//! 这样：
//!
//! - 通用 unit 机制（`With<Unit>` 的 system）自动覆盖玩家，不会漏。
//! - Player 专属 system 用 `With<Player>` filter，跟 AI / 敌人系统正交。
//!
//! # 当前只是占位
//!
//! 视觉是一个亮黄色立方体；等 sprite billboard 视觉敲定（见
//! `doc/game-design.md` §15）再换成真正的角色实体。物理参数（碰撞盒、
//! 移动速度）也只是 prototype 值。

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_asset_loader::prelude::*;
use bevy_sprite3d::prelude::*;

pub mod animation;

use super::facing::Facing;
use super::hitbox::Faction;
use super::hurtbox::spawn_hurtbox;
use super::movement::MoveVelocity;
use super::skill::{CastSkillRequest, SkillKind, SkillKindSet};
use super::{Body, Dead, Health, Strength, UNIT_BODY_HEIGHT, Unit};
use crate::app_state::AppState;
use crate::physics_layers::GameLayer;
use crate::projectile::spawn_linear_projectile;
use crate::sprite_billboard::{BillboardSprite, PIXELS_PER_METER};
use animation::{PlayerAnimationPlugin, PlayerAnimationState};

/// 玩家物理 body 半径（米）。
///
/// body 是 capsule，**总高**由共享常量 [`UNIT_BODY_HEIGHT`] 决定，这
/// 里只控制半径 = 顶视 XZ 上的推挤占位。必须 ≤ `UNIT_BODY_HEIGHT / 2`，
/// 否则 [`PLAYER_BODY_LENGTH`] 会负（capsule 无解）。选 capsule 而不是
/// sphere 是为了同享[`UNIT_BODY_HEIGHT`]的“并排接触法线纯水平”特性，
/// 不同 R 的 unit 互推时 Y 不会抖动。
///
/// 玩家落地后父 entity 位于 `y = UNIT_BODY_HEIGHT / 2`（capsule 中心
/// = 总高一半），与半径无关。
const PLAYER_BODY_RADIUS: f32 = 0.4;
/// capsule 的圆柱段长度（**不含**两端半球）—— avian `Collider::capsule`
/// 第二个参数要的就是这个。推导：总高 H = 2R + L → L = H - 2R。
const PLAYER_BODY_LENGTH: f32 = UNIT_BODY_HEIGHT - 2.0 * PLAYER_BODY_RADIUS;
/// sprite 子实体相对父实体的 Y 偏移（米）。
///
/// 配合 [`Sprite3d::pivot`] = `(0.5, 0.0)` 使用：贴图的“脚中”对齐到
/// sprite mesh 的局部 (0, 0)，所以子实体局部 (0, 0) 落在哪，sprite
/// 的“脚”就在哪。父 entity（capsule）落地后中心位于
/// `y_world = UNIT_BODY_HEIGHT / 2`，要让“脚”贴地面（`y_world = 0`），
/// 子实体局部 Y = `0 - UNIT_BODY_HEIGHT / 2`。
///
/// 这个偏移**只跟物理 body 总高有关**，跟 sprite 贴图自身像素尺寸
/// 完全解耦 —— 换贴图 / 换 sprite sheet / 换帧大小都不用动它。
const PLAYER_SPRITE_OFFSET_Y: f32 = -UNIT_BODY_HEIGHT * 0.5;
/// 玩家平移速度（米/秒）。
const PLAYER_SPEED: f32 = 5.0;
/// 玩家初始最大生命值。数值是 prototype 阶段的占位，等战斗公式立起来再调。
const PLAYER_MAX_HEALTH: f32 = 100.0;

/// 玩家用的预加载资产集合。
///
/// 由 [`bevy_asset_loader`] 在 [`AppState::Loading`] 阶段填充：宏自动
/// 生成的 `AssetCollection::create` 会 `asset_server.load_with_settings`
/// 出 handle、监控就绪、最后把这个结构体作为 `Resource` insert 进 World。
/// 因此在 [`AppState::InGame`] 的 `OnEnter` 或后续 system 里 `Res<PlayerAssets>`
/// 拿到时，`sprite` handle **保证已完成加载** —— 调用 `spawn_player` 不用再担
/// 心“贴图还在路上”。
///
/// # 采样器：nearest
///
/// `#[asset(image(sampler(filter = nearest)))]` 等同于手写 `ImageSamplerDescriptor::nearest()`
/// —— 保留像素边缘锐利，不让 linear 插值把像素艺术糊掉。
#[derive(AssetCollection, Resource)]
pub struct PlayerAssets {
    /// 玩家合表后的总 sprite sheet。
    ///
    /// 由外部工具 [`crates/tools/src/bin/stitch_player_sprites.rs`] 把
    /// `idle.png` / `run.png` / `attack.png` / `jump.png` 横向拼成单张
    /// 2560×64 的 PNG（20 帧 × 128×64）。**改源图后必须重跑 stitcher**
    /// 才能让游戏看到改动。
    ///
    /// 为什么不直接加载 4 张分动作图：`bevy_sprite3d` 在 entity spawn
    /// 时把每帧 UV 烤进 mesh 缓存（UV 还依赖整张图尺寸），单 sprite
    /// entity 只能绑一张图。合表 + 单一 [`TextureAtlasLayout`] 让同一
    /// entity 切动作 = 改 frame index，零 entity 重生成开销。
    #[asset(path = "sprites/units/player/sheet.png")]
    #[asset(image(sampler(filter = nearest)))]
    pub sheet: Handle<Image>,

    /// 描述 [`sheet`](Self::sheet) 上每帧位置的 atlas layout。
    ///
    /// 20 列 × 1 行，单帧 128×64 —— 顺序与 stitcher 的 `ACTIONS`
    /// 列表对齐，具体 index 区间在
    /// [`animation::PlayerAction::range`] 里硬编码。
    #[asset(texture_atlas_layout(tile_size_x = 128, tile_size_y = 64, columns = 20, rows = 1))]
    pub layout: Handle<TextureAtlasLayout>,
}

/// 玩家标记。
///
/// `#[require(...)]` 是 Bevy 0.15+ 的 required components 机制：spawn `Player`
/// 时 Bevy 自动挂上这些依赖组件 —— 语义上等于"`Player` 是一种 `Unit`，
/// 且无需手写的生命值初始为满血"。实现上是组合而非继承：组件都挂在
/// 同一 entity 上。
#[derive(Component, Debug, Default)]
#[require(
    Unit,
    Body,
    Health = Health::new(PLAYER_MAX_HEALTH),
    PlayerAnimationState,
    Facing,
)]
pub struct Player;

/// Player 插件 —— 注册键盘移动 system，以及 debug build 下的调试快捷键。
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // 把 PlayerAssets 挂到 AppState::Loading 阶段的 LoadingState 上。
        // 这一步要求 `AppStatePlugin` 已经 add 过 —— 见 lib.rs 里的注册顺序。
        // 等所有挂在此 LoadingState 上的 collection 都就绪，bevy_asset_loader
        // 会自动把 `PlayerAssets` insert 成 Resource，并把 state 切到 InGame。
        app.configure_loading_state(
            LoadingStateConfig::new(AppState::Loading).load_collection::<PlayerAssets>(),
        );

        // 动画状态机 —— 放在子 plugin 里、调度在 PostUpdate，详见
        // `animation.rs` 模块文档。
        app.add_plugins(PlayerAnimationPlugin);

        // 玩家移动：每帧根据输入写 `LinearVelocity`，物理在 `FixedPostUpdate`
        // 64Hz 积分。配合 entity 上的 `TransformInterpolation`，资产同帧间插值到
        // 渲染帧率，避开「物理 tick 跳跳」造成的可见顿温。Update 里写位
        // FixedUpdate 里写都可以：`pressed` 是连续状态，多次写同一个值无损失，
        // Update 频率高一点起码保证下个物理 tick 看到的是最新输入。
        app.add_systems(Update, player_movement);

        // J 键 → 发 [`CastSkillRequest`] 请求放 BasicMeleeSlash。这里只是输入到
        // 意图的最薄一层；技能能不能起手（cooldown / 已在施法）由 SkillPlugin
        // 的 `try_start_requested_casts` 判断。还没有正式的输入映射层，先把
        // 键位写死在这里，等真做按键绑定再抽。
        app.add_systems(Update, player_cast_basic_melee_on_j);

        // debug 调试快捷键仅 debug 构建编译，release / dist 零运行开销。
        // 现阶段还没有实际的伤害源（敌人 / 陷阱），这两个键位用来手动
        // 验证 Health / Dead / 复活 的路径是否走通。
        #[cfg(debug_assertions)]
        app.add_systems(
            Update,
            (
                debug_damage_player_on_space,
                debug_revive_player_on_r,
                debug_fire_projectile_on_f,
            ),
        );
    }
}

/// 在指定 stage 下 spawn 玩家，返回 player entity。
///
/// 玩家作为 `parent_stage` 的子实体（通过 [`ChildOf`] relationship），
/// 这样 stage despawn 时玩家自动连带销毁；`local_pos` 是相对 stage 局部
/// 坐标系的初始位置。
///
/// # 视觉 vs 物理拆开
///
/// player entity 本身只挂物理 / 逻辑（Collider / Health / RigidBody），
/// 不挂 mesh。视觉部分是一个子实体：带 [`BillboardSprite`] 的 2D 贴片，
/// LocalTransform 抬高使“脚”贴地面。这样 sprite 尺寸跟物理盒尺寸互
/// 不干扰，未来加影子 sprite / 武器 sprite 也就是多加几个子实体的事。
///
/// # 参数
///
/// - `player_assets`：[`PlayerAssets`] 资源，由 LoadingState 保证就绪
/// - `parent_stage`：[`spawn_stage`](crate::stage::spawn_stage) 返回的根 entity
/// - `local_pos`：stage 局部坐标系下的初始位置（Y > 0 让玩家从空中落下）
pub fn spawn_player(
    commands: &mut Commands,
    player_assets: &PlayerAssets,
    parent_stage: Entity,
    local_pos: Vec3,
) -> Entity {
    let player_entity = commands
        .spawn((
            Player,
            // 玩家会的招式。SkillKindSet 是唯一应该写的 intent；SkillBook
            // （当前数值缓存）和 SkillCooldowns 由 `#[require]` 自动带上，
            // recompute 系统在首帧 / source 变化时自动填好 SkillBook。
            SkillKindSet::new([SkillKind::BasicMeleeSlash]),
            // caster-side 输出系数 —— 被 `recompute_skill_book` 读取、烧进
            // SkillBook 里每招的 HitSpec.modifiers 头部。玩家初始 1.0
            // （无加成），未来装备 / buff 会修改。
            Strength(1.0),
            // 阵营 —— 技能 / hitbox / 友冷过滤 未来都会读。玩家总是 Player 阵营。
            Faction::Player,
            Transform::from_translation(local_pos),
            // 父 entity 自身不渲染，但 sprite 子 entity 带 `Visibility`
            // （`Sprite3d` 隐式 require）。Bevy 的可见性沿 hierarchy 继承，
            // 父没有 `Visibility` → `InheritedVisibility` 传播链断 → B0004 warning。
            // 加一个默认值把链补上；同时未来想整角色统一隐藏也能直接 toggle 这里。
            Visibility::default(),
            // Kinematic 刚体：position / velocity / 重力 全部由游戏代码接管
            // （见 `unit/movement.rs` 顶部文档）。走动不是被 solver
            // 推出来的，是每帧 sweep-and-slide 主动推出来的 —— 互相挡却
            // 互不推动，适合 top-down ARPG 的 go-stop 手感。
            RigidBody::Kinematic,
            // capsule body：其他 unit 不同半径互推时接触点落在圆柱中段、
            // 法线纯水平，Y 方向不抽。总高 [`UNIT_BODY_HEIGHT`] 为全场
            // ground unit 共享，这里只需在调用点拼出 cylinder 段长度。
            Collider::capsule(PLAYER_BODY_RADIUS, PLAYER_BODY_LENGTH),
            // CollisionLayers：membership = Body，filter = [Body, Terrain]。
            // body 只跟其他 unit body 和地形互推，跳过 hurtbox / hitbox
            // —— 避免被自己的受击盒顶起来、避免被友军攻击 sensor 推开。
            CollisionLayers::new(GameLayer::Body, [GameLayer::Body, GameLayer::Terrain]),
            // 防止被撞翻滚 —— 俯视斜角游戏角色应保持站立。不锁
            // 转动会被击飞 / 撞压之类的接触带动。Kinematic 下其实
            // solver 不会主动转动我们，但保留表达意图。
            LockedAxes::ROTATION_LOCKED,
            // Bevy 0.18 relationship API：把自己挂成 parent_stage 的子实体。
            ChildOf(parent_stage),
        ))
        .id();

    // sprite 子实体：独立视觉结点。`Sprite3d` (bevy_sprite3d) 在 PostUpdate
    // 的 bundle_builder system 里读 `Sprite.image` 尺寸自动生成 quad mesh +
    // 配套的 `StandardMaterial`（`alpha_mode` 默认 Mask(0.5)、`double_sided=true`
    // 替代我们以前手写的 `cull_mode: None`）。我们这里只挂“数据”：图片、
    // 像素密度、unlit、pivot；mesh / material 资产由 sprite3d 内部缓存，多
    // entity 共享。
    //
    // pivot=(0.5, 0.0)：让贴图的“脚中”对齐到 sprite mesh 局部 (0, 0)，
    // 详见 [`PLAYER_SPRITE_OFFSET_Y`] 文档。
    commands.spawn((
        BillboardSprite,
        Sprite3d {
            pixels_per_metre: PIXELS_PER_METER,
            unlit: true,
            pivot: Some(Vec2::new(0.5, 0.0)),
            ..default()
        },
        // 用 atlas 形态：bevy_sprite3d 看到 `Sprite.texture_atlas`
        // 是 `Some` 时，会按 layout 里的每帧分别预烤一张共享 mesh，
        // 之后切动作只需要改 `texture_atlas.index`，mesh 由
        // `handle_texture_atlases` system 自动换。
        Sprite::from_atlas_image(
            player_assets.sheet.clone(),
            TextureAtlas {
                layout: player_assets.layout.clone(),
                index: 0,
            },
        ),
        Transform::from_translation(Vec3::new(0.0, PLAYER_SPRITE_OFFSET_Y, 0.0)),
        ChildOf(player_entity),
    ));

    // 受击判定：现阶段用跟 body 同型的 capsule 作为整块受击区，
    // 简单覆盖角色。未来要分头 / 身 / 腿不同倍率时可多次调
    // `spawn_hurtbox` 贴多块，或者让 hurtbox transform 随动作变。
    // 当前不预先抽象。
    spawn_hurtbox(
        commands,
        player_entity,
        Collider::capsule(PLAYER_BODY_RADIUS, PLAYER_BODY_LENGTH),
        Transform::IDENTITY,
    );

    player_entity
}

/// 方向键移动玩家：每帧根据按键设置 X/Z 方向期望速度、写入 [`MoveVelocity`]，
/// Y 留给 [`apply_gravity`](crate::unit::movement)。
///
/// 为什么写 [`MoveVelocity`] 而不是直接改 Transform：输入是"期望走多远"，
/// 能不能走、能走多远由 sweep-and-slide 加上环境约束决定。玩家不应该直
/// 接改 Position，否则会穿模、穿墙、跳过另一个 unit。
///
/// 为什么不写 avian 的 `LinearVelocity`：见 [`MoveVelocity`] 文档 ——
/// 简言之，避免与 avian 位置集成器双重位移。
///
/// 朝向约定：相机在 +Y +Z 看向原点（见 `lib.rs::spawn_camera`），所以屏幕
/// 上"远端 = -Z"。WASD 留给 dev 相机（见 `dev_camera.rs`），玩家用方向键。
///
/// - `↑` → -Z（向屏幕远端走）
/// - `↓` → +Z（朝相机走）
/// - `←` → -X（左）
/// - `→` → +X（右）
///
/// `Without<Dead>` 是 unit 模块的全局约定（见该模块顶部文档）：死了的
/// unit 不走移动逻辑，躺到原地。
fn player_movement(
    keys: Res<ButtonInput<KeyCode>>,
    mut q_player: Query<&mut MoveVelocity, (With<Player>, Without<Dead>)>,
) {
    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowUp) {
        input.y -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowLeft) {
        input.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        input.x += 1.0;
    }
    let target_xz = if input.length_squared() > 0.0 {
        input.normalize() * PLAYER_SPEED
    } else {
        Vec2::ZERO
    };

    for mut vel in &mut q_player {
        // 只覆盖 X / Z；Y 留给重力 / 击飞 impulse 之类的其他来源。
        vel.0.x = target_xz.x;
        vel.0.z = target_xz.y;
    }
}

/// 按 `Space` 给玩家一次 10 点伤害 —— 这是**调试合成伤害**，直接写
/// [`Health::current`]，**不走**正规的 [`DamagePipeline`](super::DamagePipeline)
/// 流水线：没有 caster / hitbox 来源，伪造一条 `DamageDealtMessage` 反而
/// 让下游的 trigger / 统计系统看到一个虚假 caster Entity，多一层坑。
/// 调试键就应该是"跳过中间环节、直接验证结果"。
///
/// `Without<Dead>` filter —— 死人不再被打（跟 pipeline 一致）。打死后按
/// `R` 复活才能继续測试。
///
/// 只在 debug build 编译；release / dist 完全不存在这个 system。
#[cfg(debug_assertions)]
fn debug_damage_player_on_space(
    keys: Res<ButtonInput<KeyCode>>,
    mut q_player: Query<&mut Health, (With<Player>, Without<Dead>)>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    for mut hp in &mut q_player {
        hp.current = (hp.current - 10.0).max(0.0);
    }
}

/// 按 `R` 让玩家"满血复活"：清掉 [`Dead`] marker 并把 [`Health::current`]
/// 拉回 [`Health::max`]。注意没有 `Without<Dead>` filter —— 复活就是要
/// 对死了的人也生效。
///
/// 只在 debug build 编译。
#[cfg(debug_assertions)]
fn debug_revive_player_on_r(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut q_player: Query<(Entity, &mut Health), With<Player>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    for (entity, mut health) in &mut q_player {
        health.current = health.max;
        // remove::<Dead>() 对没挂 Dead 的 entity 也安全 —— Bevy 静默忽略。
        commands.entity(entity).remove::<Dead>();
    }
}

/// 按 `F` 发射一发匀速直线投射物 —— 绕开未来的技能 / 输入映射层，
/// 直接验证投射物子系统打通（spawn → 飞行 → 命中 hurtbox / terrain →
/// despawn）。方向取自 [`Facing`]：右 = +X，左 = -X。
///
/// 只在 debug build 编译。
///
/// `clippy::type_complexity`：跟 movement.rs 同款理由 —— Bevy Query 的类型
/// 参数堆起来就是这副长相，已是项目惯例。
#[cfg(debug_assertions)]
#[allow(clippy::type_complexity)]
fn debug_fire_projectile_on_f(
    keys: Res<ButtonInput<KeyCode>>,
    q_player: Query<(Entity, &Transform, &Facing), (With<Player>, Without<Dead>)>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    for (entity, transform, facing) in &q_player {
        // Facing 在动画 / 视觉层就是横向二值朝向；投射物方向直接 X 轴。
        // 这里没经过 facing.rs 的 helper —— 那种 "Facing → Vec3" 是
        // 视觉用语义（往哪边看），不一定等于"投射物速度方向"（未来
        // 可能从角色手中往斜上方射出）。spawn 调用方明示更清楚。
        let dir = match facing {
            Facing::Right => Vec3::X,
            Facing::Left => Vec3::NEG_X,
        };
        // 出膛位置：略前于角色，避免 spawn 当帧就命中自己（其实 hitbox
        // 子系统的 owner 过滤已经会拦，但视觉上让它从身前飞出更自然）。
        let origin = transform.translation + dir * 0.5;
        let velocity = dir * 10.0; // 占位速度，调参等真出现远程攻击再说。
        spawn_linear_projectile(
            &mut commands,
            entity,
            Faction::Player,
            origin,
            velocity,
            15.0, // 占位伤害
            3.0,  // 占位寿命（秒）
        );
    }
}

/// 按 `J` 发起 BasicMeleeSlash 施法请求。
///
/// 只发请求，不直接开始施法 —— [`SkillPlugin`](super::skill::SkillPlugin) 的
/// `try_start_requested_casts` 检查 cooldown / 是否已在施法 / 是否拥有该技能，
/// 通过后才挂上 [`SkillCast`](super::skill::SkillCast) 进入 windup。
///
/// 死了的玩家不接受输入。
fn player_cast_basic_melee_on_j(
    keys: Res<ButtonInput<KeyCode>>,
    q_player: Query<Entity, (With<Player>, Without<Dead>)>,
    mut requests: MessageWriter<CastSkillRequest>,
) {
    if !keys.just_pressed(KeyCode::KeyJ) {
        return;
    }
    for caster in &q_player {
        requests.write(CastSkillRequest {
            caster,
            kind: SkillKind::BasicMeleeSlash,
        });
    }
}
