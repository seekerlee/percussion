//! Stage（舞台）插件 —— 一片有边界的演出空间。
//!
//! # 心智模型
//!
//! Stage **不是** Bevy 内置的 [`Scene`](bevy::scene::Scene) 类型，也**不是**
//! 全局 [`States`](bevy::state::state::States)，而是一个**普通 entity**：
//!
//! - 挂 [`Stage`] 标记自己是个 stage 根
//! - 挂 [`StageBounds`] 描述自己的逻辑边界（地面矩形 + 净空高度）
//! - 挂 [`Transform`] / [`Visibility`]：作为父，子实体（地面、视觉罩、
//!   物理屏障、单位、子弹）的 world transform 沿 hierarchy 自动传播
//!
//! Stage 内的几何（地面、视觉罩、物理屏障）由 [`spawn_stage`] 直接作为
//! children spawn。后续的角色 / 子弹由外部模块（如
//! [`crate::unit::player::spawn_player`]）通过 `ChildOf(stage_entity)` 挂进来。
//! Despawn 这个 root 会通过 Bevy 0.18 relationship API 自动连带销毁所有
//! children。
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
//! # 边界三层：逻辑 / 物理 / 视觉
//!
//! Stage 的边界由**三层独立的设施**承担，尺寸都从 [`StageBounds`] 推导：
//!
//! - **逻辑边界** ([`StageBounds`])：stage 在自身局部坐标系下占据的 3D 盒子。
//!   供 system 当查询锚点 —— AI 巡逻 / 站位、多 stage 归属判定都用它。
//! - **物理屏障**：6 面闭合的 [`RigidBody::Static`] cuboid collider
//!   （4 立面 + 顶 + 地），尺寸跟 [`StageBounds`] 完全对齐。**兜底**用 ——
//!   玩家 / 子弹 / 怪物等不可能穿出 stage。Stage 内部还可以另外加物理
//!   障碍，本模块不管。
//! - **视觉罩**：5 面半透明蓝白 plane（4 立面 + 顶）+ 1 面不透明绿色地面，
//!   组成"6 面盒子"外观。视觉罩**只是 mesh，不带 collider** —— 视觉跟
//!   物理彻底分开，方便后面单独换皮。
//!
//! 因为物理已经兜底，子弹**不需要**靠逻辑越界 despawn 来回收 —— 想飞出去
//! 自然会被 bounds 内壁挡住；真正打不到的子弹靠生命周期 / 撞击事件管。

use avian3d::prelude::*;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

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
/// - AI 巡逻 / 站位决策的几何参考
/// - 多 stage 共存时判定 entity 归属
///
/// # 与物理 / 视觉设施的关系
///
/// `StageBounds` 跟 spawn 时构造的物理 collider、视觉 plane 的尺寸
/// **完全对齐** —— [`spawn_stage`] 用 `size` / `height` 直接算出 6 面盒子
/// 的几何。改 `StageBounds` 等于改整个 stage 的物理 + 视觉外形。
#[derive(Component, Debug, Clone, Copy)]
pub struct StageBounds {
    /// 地面矩形的**全尺寸**（X / Z 方向的总长，米）。
    pub size: Vec2,
    /// 从地面（Y=0）到逻辑顶的高度（米）。
    pub height: f32,
}

/// 物理屏障 cuboid 厚度（米）。
///
/// 屏障是 [`StageBounds`] 边界外侧的隐形 collider（无 mesh）。厚度只要 >
/// 单帧最大移动距离就能防穿透；过厚浪费物理求解时间。
const BARRIER_THICKNESS: f32 = 0.5;

/// Stage 插件 —— 提供 stage **能力**（capability）：组件、spawn API。
///
/// # 职责
///
/// - 暴露 [`Stage`] / [`StageBounds`] 组件
/// - 暴露 [`spawn_stage`] 函数，让调用方决定何时何地 spawn
///
/// # 依赖
///
/// 假设上游（`GamePlugin`）已经注册了 [`PhysicsPlugins`] —— 物理是
/// 引擎层基础设施，stage 只是它的消费者之一。
///
/// # 不负责
///
/// - 决定"开局 spawn 哪个 stage / 多大 / 在哪"（由 `GamePlugin` 的
///   startup 决定 —— 这是游戏 policy，不是 stage 能力）
/// - 在 stage 里放角色（玩家由 [`crate::unit::player`] 管，敌人 / NPC 由
///   各自模块管，stage 只提供"空舞台"）
/// - 相机摆位（由 `lib.rs` 的 `spawn_camera` 管）
/// - 灯光（由 `lib.rs` 管，跟相机同层级 —— 渲染前置条件）
/// - debug 可视化（由 `debug` 模块管）
/// - 怪物 / 子弹 / 触发区（后续 plugin 接入时再加）
pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, _app: &mut App) {
        // Stage 目前是纯数据 + spawn API，没有需要每帧跑的 system。
        // 保留 plugin 形态是为了跟项目"一个 module 一个 plugin"模式对齐。
    }
}

/// 在 `origin` 位置 spawn 一个 stage，返回 stage 根 entity 句柄。
///
/// Stage 内部的几何（地面、墙）作为根的 children spawn，它们的
/// transform 是 stage 局部坐标系下的 —— stage 根的 transform 决定
/// 整个 stage 在 world 里的宏观位置（用于多 stage 空间分离）。
///
/// 角色（玩家 / NPC / 敌人）不在这里 spawn —— 它们是独立的演员，
/// 由调用方拿到返回的 stage entity 后，用各自模块的 spawn 函数
/// （如 [`crate::unit::player::spawn_player`]）挂进来。
///
/// # 参数
///
/// - `size`：地面矩形全尺寸（X × Z 全长，米）
/// - `height`：盒子净空高度（米）—— 视觉罩立面 / 顶面、物理屏障顶面都
///   按这个高度算
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

    // 视觉罩材质：淡蓝半透明玻璃。
    //
    // 为什么**不用** `cull_mode: None`：
    // 由 5 个贴壳的 plane（4 立面 + 顶）组成的罩，如果两面都渲染
    // 会在 Bevy 透明队列中制造 10 个 bounding box 中心位置几乎重叠的
    // entity，以距离排序时数值拖动→ 渲染顺序乱 → 画面闪烁或看不见，
    // 同时 overdraw 翻倍。改用单面渲染 (默认 cull Back) + **法线指向
    // stage 内部**：从相机位置看，远端 / 左右 / 顶 4 面的正面朋相机可见；
    // 近端立面是背面被剪（不遮玩家）。每个像素最多 1 层透明 fragment，
    // 无 sort 抖动。
    let cover_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.75, 1.00, 0.30),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // 5 面半透明罩 (4 立面 + 顶) 的 (plane mesh 尺寸, 摆放 transform)，
    // 5 面物理屏障 (4 立面 + 顶) 的 (collider 中心, 全尺寸)。在闭包外算好
    // 各 mesh handle，避免在 with_children 里反复借 meshes。
    let cover_specs = cover_plane_specs(size, height);
    let cover_meshes: [Handle<Mesh>; 5] = std::array::from_fn(|i| {
        let s = cover_specs[i].0;
        meshes.add(Plane3d::default().mesh().size(s.x, s.y))
    });
    let barriers = barrier_specs(size, height);

    commands
        .spawn((
            Stage,
            StageBounds { size, height },
            Transform::from_translation(origin),
            Visibility::default(),
        ))
        .with_children(|stage| {
            // 地面：不透明绿色 mesh + 静态薄 collider。
            //
            // Plane3d mesh 本身是零厚度的 quad；为了让物理引擎能稳定地接住
            // 落下来的角色，再加一个 2cm 厚的 cuboid collider。1cm 的厚度
            // 误差视觉上完全看不见。地面 collider 同时充当 bounds 6 面屏障
            // 的"底面" —— 不在下面 `barriers` 数组里另外算。
            stage.spawn((
                Mesh3d(ground_mesh),
                MeshMaterial3d(ground_material),
                Transform::default(),
                RigidBody::Static,
                Collider::cuboid(size.x, 0.02, size.y),
            ));

            // 5 面半透明罩 (4 立面 + 顶)：纯 mesh，不带 collider。
            for (i, spec) in cover_specs.iter().enumerate() {
                stage.spawn((
                    Mesh3d(cover_meshes[i].clone()),
                    MeshMaterial3d(cover_material.clone()),
                    spec.1,
                ));
            }

            // 5 面物理屏障 (4 立面 + 顶)：纯 collider，没有 mesh。
            for (center, full_size) in barriers {
                stage.spawn((
                    Transform::from_translation(center),
                    RigidBody::Static,
                    Collider::cuboid(full_size.x, full_size.y, full_size.z),
                ));
            }
        })
        .id()
}

/// 计算 5 面半透明罩 (4 立面 + 顶) 的 (plane mesh 尺寸, 摆放 transform)。
///
/// Bevy 的 [`Plane3d`] 默认是 XZ 平面 (法线 +Y)。每个立面 / 顶面的旋转
/// **必须让法线指向 stage 内部** —— 这样单面渲染 (默认 cull Back) 时，
/// 相机从 stage 外看过去看到的是远端面的正面（可见），近端立面被背面
/// 剪除（不遮玩家）。这是“玻璃展示柜”的视觉逻辑。
///
/// Mesh 的 `size(w, h)` 参数：旋转**前** X 方向 = w、Z 方向 = h，旋转后
/// 立面上的"水平宽 / 竖直高"映射到哪个轴见各分支注释。
///
/// 视觉布局（俯视）：
///
/// ```text
///   北 (z 负方向)
/// ┌─────────────────┐
/// │                 │
/// │西             东│
/// │                 │
/// └─────────────────┘
///   南 (z 正方向)
/// ```
///
/// 本项目坐标约定：XZ 是地面，相机在 +Z 方向看向原点。"北 = -Z" 是屏幕
/// "远端"，"南 = +Z" 是屏幕"近端"。
fn cover_plane_specs(size: Vec2, height: f32) -> [(Vec2, Transform); 5] {
    let hx = size.x / 2.0;
    let hz = size.y / 2.0;
    [
        // 北立面 (z = -hz)：绕 X 轴 +90° 把 +Y 法线转到 +Z，
        // mesh (X 宽, Z 高) → 立面 (X 宽, Y 高)。
        (
            Vec2::new(size.x, height),
            Transform::from_xyz(0.0, height / 2.0, -hz)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        ),
        // 南立面 (z = +hz)：绕 X 轴 -90° 把 +Y 法线转到 -Z（指向 stage 内）。
        // 这面在近端，背面被剪后从相机看过去不遮玩家。
        (
            Vec2::new(size.x, height),
            Transform::from_xyz(0.0, height / 2.0, hz)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ),
        // 西立面 (x = -hx)：绕 Z 轴 -90° 把 +Y 法线转到 +X（指向 stage 内）。
        // mesh (X 宽, Z 高) → 立面 (Y 高, Z 宽)。
        (
            Vec2::new(height, size.y),
            Transform::from_xyz(-hx, height / 2.0, 0.0)
                .with_rotation(Quat::from_rotation_z(-FRAC_PI_2)),
        ),
        // 东立面 (x = +hx)：绕 Z 轴 +90° 把 +Y 法线转到 -X（指向 stage 内）。
        (
            Vec2::new(height, size.y),
            Transform::from_xyz(hx, height / 2.0, 0.0)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
        ),
        // 顶面 (y = height)：绕 X 轴 180° 把 +Y 法线翻到 -Y（指向 stage 内）。
        // 相机从斜上方俯视 → 看到顶面内侧的正面，透明可见。
        (
            Vec2::new(size.x, size.y),
            Transform::from_xyz(0.0, height, 0.0).with_rotation(Quat::from_rotation_x(PI)),
        ),
    ]
}

/// 计算 5 面物理屏障 (4 立面 + 顶) 的 (cuboid 中心, 全尺寸)。
///
/// 屏障是 [`RigidBody::Static`] + [`Collider::cuboid`]，**没有 mesh**。
/// 贴在 [`StageBounds`] 边界**外侧**（中心比 bounds 多偏出半个厚度），
/// 视觉上玩家撞到屏障时罩 plane 还正好贴着 bounds 边沿。
///
/// 立面横跨 X 方向时**含两端的厚度** (`size.x + 2 * t`)，立面跨 Z 方向
/// 时**不含**两端厚度 —— 这样四角不重叠。顶面同时含 X / Z 两侧厚度。
///
/// 地面屏障由 ground 实体自带的 collider 充当，不在此处返回。
fn barrier_specs(size: Vec2, height: f32) -> [(Vec3, Vec3); 5] {
    let hx = size.x / 2.0;
    let hz = size.y / 2.0;
    let t = BARRIER_THICKNESS;
    [
        // 北 (-z 方向)
        (
            Vec3::new(0.0, height / 2.0, -hz - t / 2.0),
            Vec3::new(size.x + 2.0 * t, height, t),
        ),
        // 南 (+z 方向)
        (
            Vec3::new(0.0, height / 2.0, hz + t / 2.0),
            Vec3::new(size.x + 2.0 * t, height, t),
        ),
        // 西 (-x 方向)
        (
            Vec3::new(-hx - t / 2.0, height / 2.0, 0.0),
            Vec3::new(t, height, size.y),
        ),
        // 东 (+x 方向)
        (
            Vec3::new(hx + t / 2.0, height / 2.0, 0.0),
            Vec3::new(t, height, size.y),
        ),
        // 顶 (+y 方向)
        (
            Vec3::new(0.0, height + t / 2.0, 0.0),
            Vec3::new(size.x + 2.0 * t, t, size.y + 2.0 * t),
        ),
    ]
}
