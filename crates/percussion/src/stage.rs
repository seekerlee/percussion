//! Stage（舞台）插件 —— 一片有边界的演出空间。
//!
//! # 心智模型
//!
//! Stage **不是** Bevy 内置的 [`Scene`](bevy::scene::Scene) 类型，也**不是**
//! 全局 [`States`](bevy::state::state::States)，而是一个**普通 entity**：
//!
//! - 挂 [`Stage`] 标记自己是个 stage 根
//! - 挂 [`StageBounds`] 描述自己的逻辑边界（地面矩形 + 净空高度）
//! - 挂 [`Transform`] / [`Visibility`]：作为父，子实体（地面、墙、单位、子弹）
//!   的 world transform 沿 hierarchy 自动传播
//!
//! Stage 内的所有实体作为这个 root entity 的 children；despawn 这个 root
//! 会通过 Bevy 0.18 relationship API 上 `Children` 的 `linked_spawn`
//! 标记自动连带销毁，不需要手写循环。
//!
//! # 多 stage 同活
//!
//! 多 stage 并存时**在 world 里空间上分开**（比如第一个 stage 在原点，
//! 第二个在 (1000, 0, 0)），因为 Avian 没有"多 physics world"概念，所有
//! collider 共享同一物理空间，重叠会互相干扰。
//!
//! [`spawn_stage`] 函数接受一个 `origin: Vec3` 参数，调用方负责给不同
//! stage 分配不同的宏观位置。何时、何地、用什么尺寸 spawn 由调用方决定
//! （当前在 `GamePlugin` 的 `spawn_initial_stage` startup 里调一次）。
//!
//! # 边界：逻辑 vs 物理
//!
//! Stage 有两层边界，它们**概念上独立**：
//!
//! - **逻辑边界** ([`StageBounds`])：stage 在自身局部坐标系下占据的 3D 盒子
//!   （XZ 地面矩形 + Y 方向净空高度）。供后续 system 当查询锚点 ——
//!   越界 despawn、AI 巡逻、多 stage 归属判定都用它。
//! - **物理设施**：4 面 [`RigidBody::Static`] 矮墙 + 地面 collider。墙是
//!   spawn 时沿 XZ 边沿构造的实体，给玩家方块"撞墙"体感反馈。
//!   **没有天花板** —— 俯视斜角相机要能看进 stage 内部。
//!
//! Spawn 时墙的 XZ 位置跟 `StageBounds.size` 对齐，但两者概念独立。
//! Y 方向边界（子弹飞过 `height`）靠**逻辑层 despawn** 维护，不是物理硬挡。

use avian3d::prelude::*;
use bevy::prelude::*;

/// 标记一个 entity 是 stage 的根。
///
/// 这个组件本身不携带数据；具体边界 / 内容由同实体上的其他组件描述。
#[derive(Component, Debug)]
pub struct Stage;

/// Stage 的**逻辑边界** —— 描述演出空间的几何身份。
///
/// 在 stage 自身局部坐标系下，演出空间占据一个盒子：
/// - XZ 平面：`[-size.x / 2, +size.x / 2] × [-size.y / 2, +size.y / 2]`
/// - Y 方向：`[0, height]`（地面在 Y=0，向上为正）
///
/// # 用途
///
/// 给后续 system 当查询锚点。例如：
/// - 越界 despawn（子弹飞过 `height` 或越出 XZ）
/// - AI 巡逻 / 站位决策的几何参考
/// - 多 stage 共存时判定 entity 归属
///
/// # 与物理设施的关系
///
/// `StageBounds` **不是**物理墙的尺寸 —— 墙是 spawn 时另外构造的物理实体，
/// 墙高用 `WALL_HEIGHT` 常量（视觉上的矮墙），跟 `height` 无关。物理上没有
/// 天花板；超出 `height` 的处理交给逻辑层。
#[derive(Component, Debug, Clone, Copy)]
pub struct StageBounds {
    /// 地面矩形的**全尺寸**（X / Z 方向的总长，米）。
    pub size: Vec2,
    /// 从地面（Y=0）到逻辑顶的高度（米）。
    pub height: f32,
}

/// 玩家方块标记。
///
/// 占位用，等 sprite billboard 视觉敲定再换成真正的角色实体。
#[derive(Component, Debug)]
pub struct Player;

/// 墙体厚度（米）。
const WALL_THICKNESS: f32 = 0.5;
/// 物理矮墙高度（米）—— **跟 [`StageBounds::height`] 无关**。
/// 给玩家方块"撞墙"体感反馈用的实体设施高度，不是 stage 的逻辑边界。
const WALL_HEIGHT: f32 = 2.0;
/// 玩家方块边长（米）。
const PLAYER_SIZE: f32 = 1.0;
/// 玩家平移速度（米/秒）。
const PLAYER_SPEED: f32 = 5.0;

/// Stage 插件 —— 提供 stage **能力**（capability）：组件、spawn API、
/// 玩家移动行为。
///
/// # 职责
///
/// - 暴露 [`Stage`] / [`StageBounds`] / [`Player`] 组件
/// - 暴露 [`spawn_stage`] 函数，让调用方决定何时何地 spawn
/// - Update 时跑玩家 WASD 移动
///
/// # 依赖
///
/// 假设上游（`GamePlugin`）已经注册了 [`PhysicsPlugins`] —— 物理是
/// 引擎层基础设施，stage 只是它的消费者之一。
///
/// # 不负责
///
/// - 决定"开局 spawn 哪个 stage / 多大 / 在哪"（由 `GamePlugin` 的
///   `spawn_initial_stage` 决定 —— 这是游戏 policy，不是 stage 能力）
/// - 相机摆位（由 `lib.rs` 的 `spawn_camera` 管）
/// - 灯光（由 `lib.rs` 管，跟相机同层级 —— 渲染前置条件）
/// - debug 可视化（由 `debug` 模块管）
/// - 怪物 / 子弹 / 触发区（后续 plugin 接入时再加）
pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, player_movement);
    }
}

/// 在 `origin` 位置 spawn 一个 stage，返回 stage 根 entity 句柄。
///
/// Stage 内部的所有几何（地面、墙、初始单位）作为根的 children spawn，
/// 它们的 transform 是 stage 局部坐标系下的 —— stage 根的 transform 决定
/// 整个 stage 在 world 里的宏观位置（用于多 stage 空间分离）。
///
/// # 参数
///
/// - `size`：地面矩形全尺寸（X × Z 全长，米）
/// - `height`：逻辑顶高（米）—— 给越界判定用；物理墙不使用这个高度
pub fn spawn_stage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    origin: Vec3,
    size: Vec2,
    height: f32,
) -> Entity {
    // 资源（mesh / material）创建在 closure 外，方便在 with_children 里
    // 直接 move 进去，避免在 closure 里反复借 `meshes` / `materials`。
    let ground_mesh = meshes.add(Plane3d::default().mesh().size(size.x, size.y));
    let ground_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.46, 0.30),
        ..default()
    });

    let wall_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.6, 0.6, 0.65),
        ..default()
    });

    let player_mesh = meshes.add(Cuboid::from_size(Vec3::splat(PLAYER_SIZE)));
    let player_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.00, 0.95, 0.36),
        ..default()
    });

    // 4 面墙的尺寸事先算好，避免在循环里散逻辑。
    let walls = wall_segments(size);
    // 4 面墙用各自独立的 mesh handle（尺寸不同，没法共享）；material 共享。
    // 闭包参数用 `wall_size`，避免跟外层 `size: Vec2` 阴影。
    let wall_meshes: [Handle<Mesh>; 4] =
        walls.map(|(_, wall_size)| meshes.add(Cuboid::from_size(wall_size)));

    commands
        .spawn((
            Stage,
            StageBounds { size, height },
            Transform::from_translation(origin),
            Visibility::default(),
        ))
        .with_children(|stage| {
            // 地面：薄静态碰撞体 + Plane3d 单色 mesh。
            //
            // Plane3d mesh 本身是零厚度的 quad；为了让物理引擎能稳定地接住
            // 落下来的玩家，再加一个 2cm 厚的 cuboid collider。1cm 的厚度
            // 误差视觉上完全看不见。
            stage.spawn((
                Mesh3d(ground_mesh),
                MeshMaterial3d(ground_material),
                Transform::default(),
                RigidBody::Static,
                Collider::cuboid(size.x, 0.02, size.y),
            ));

            // 4 面墙：static collider + 单色 cuboid mesh。
            for (i, (center_xz, wall_size)) in walls.into_iter().enumerate() {
                stage.spawn((
                    Mesh3d(wall_meshes[i].clone()),
                    MeshMaterial3d(wall_material.clone()),
                    Transform::from_xyz(center_xz.x, WALL_HEIGHT / 2.0, center_xz.y),
                    RigidBody::Static,
                    Collider::cuboid(wall_size.x, wall_size.y, wall_size.z),
                ));
            }

            // 玩家方块：dynamic 刚体，靠重力落到地面，靠墙阻挡 XZ 移动。
            // ROTATION_LOCKED 防止玩家被撞翻滚（俯视斜角游戏角色应保持站立）。
            stage.spawn((
                Player,
                Mesh3d(player_mesh),
                MeshMaterial3d(player_material),
                Transform::from_xyz(0.0, 5.0, 0.0),
                RigidBody::Dynamic,
                Collider::cuboid(PLAYER_SIZE, PLAYER_SIZE, PLAYER_SIZE),
                LockedAxes::ROTATION_LOCKED,
            ));
        })
        .id()
}

/// 计算 4 面墙的 (XZ 中心偏移, 全尺寸 Vec3) —— Y 上是墙高的一半作为 center。
///
/// 墙体是空心矩形外圈，包住 stage 边界外侧：
///
/// ```text
///   北墙 (z 负方向外)
/// ┌─────────────────┐
/// │                 │
/// │西墙          东墙│
/// │                 │
/// └─────────────────┘
///   南墙 (z 正方向外)
/// ```
///
/// 注：本项目坐标约定 XZ 是地面，相机在 +Z 方向看向原点。"北 = -Z" 是
/// 屏幕"远端"，"南 = +Z" 是屏幕"近端"。
fn wall_segments(size: Vec2) -> [(Vec2, Vec3); 4] {
    // 墙体中心 XZ 坐标在 stage 边外，用半边长写起来更短；从全长一次性除好。
    let hx = size.x / 2.0;
    let hz = size.y / 2.0;
    let t = WALL_THICKNESS;
    let h = WALL_HEIGHT;
    // N/S 墙横跨 X 方向（含两端的厚度），E/W 墙只覆盖 stage 内的 Z 范围
    // —— 这样四个角不会双重重叠。
    [
        // 北墙
        (
            Vec2::new(0.0, -hz - t / 2.0),
            Vec3::new(size.x + t * 2.0, h, t),
        ),
        // 南墙
        (
            Vec2::new(0.0, hz + t / 2.0),
            Vec3::new(size.x + t * 2.0, h, t),
        ),
        // 西墙
        (Vec2::new(-hx - t / 2.0, 0.0), Vec3::new(t, h, size.y)),
        // 东墙
        (Vec2::new(hx + t / 2.0, 0.0), Vec3::new(t, h, size.y)),
    ]
}

/// WASD 移动玩家：每帧根据按键设置 X/Z 方向线速度，Y 由重力管。
///
/// 朝向约定：相机在 +Y +Z 看向原点（见 `lib.rs::spawn_camera`），所以屏幕
/// 上"远端 = -Z"。
///
/// - `W` → -Z（向屏幕远端走）
/// - `S` → +Z（朝相机走）
/// - `A` → -X（左）
/// - `D` → +X（右）
fn player_movement(
    keys: Res<ButtonInput<KeyCode>>,
    mut q_player: Query<&mut LinearVelocity, With<Player>>,
) {
    let mut input = Vec2::ZERO;
    let target_xz = if input.length_squared() > 0.0 {
        input.normalize() * PLAYER_SPEED
    } else {
        Vec2::ZERO
    };

    for mut vel in &mut q_player {
        // 只覆盖 X / Z；Y 留给重力，玩家会自然贴着地面。
        vel.x = target_xz.x;
        vel.z = target_xz.y;
    }
}
