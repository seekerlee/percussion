//! Billboard sprite —— 让 2D 贴片在 3D 场景里始终正面对屏幕。
//!
//! # 这个模块解决什么问题
//!
//! 项目的视觉路线是 **3D 世界 + 2D billboard sprite**（饥荒 / Delver /
//! The Last Night 都是这套）：相机以斜俯角看 3D 场景，但角色、道具、
//! 特效都是平面 2D 贴图，靠每帧把贴图的姿态对齐相机来"伪装"成立体。
//!
//! 这个模块只管"朝向相机"这一件事 —— 不管贴图怎么挑、mesh 怎么建、
//! material 怎么配。调用方 spawn 一个 [`Mesh3d`] + [`MeshMaterial3d`]
//! 后挂上 [`BillboardSprite`] marker，剩下交给本模块的 system。
//!
//! 因为 billboard 是**渲染层**通用机制（道具、地面提示、伤害浮字以后
//! 也都会用），不属于 unit / player / stage 任一具体领域，所以独立成
//! 模块。
//!
//! # 用 Y 轴 billboard（不是 full billboard）
//!
//! 「**Full billboard**」让 sprite 的 plane 跟相机 image plane 完全平行
//! （sprite world rotation = camera world rotation）：屏幕上 sprite 永远
//! 是正立矩形。代价是 sprite 在 world 里跟着相机一起斜着躺，**跟世界
//! 垂直的 body collider 是两套坐标系**。透视相机下， body 的顶端会朝屏
//! 幕中心顶部消失点轻微倾斜（投影几何固有），而 sprite 永远屏幕正立 ——
//! 越往屏幕边缘错位越大，视觉上"sprite 跟 collider 不对齐"。
//!
//! 「**Y 轴 billboard**」（即本模块当前实现）只把 sprite 在水平面 yaw
//! 转向相机，sprite 在 world 里**始终垂直于地面**，跟 body 处于同一坐标系。
//! 透视相机下，整个 3D 世界（包括 sprite、地面网格、未来的特效）一起
//! 朝屏幕边缘消失点 lean —— 自洽，眼睛自动把它读成"哦这是 3D 透视"，
//! 错位感消失。WC3 / 饥荒 / Hades 都是这套：3D 单位配 2D 贴图，都在世界
//! 坐标里一起 lean。
//!
//! 代价：屏幕边缘的 sprite 视觉上轻微 lean（lean 角度跟 camera FOV + 屏幕
//! 偏移成正比）。当前 [`CAMERA_FOV_DEG = 30`](crate::CAMERA_FOV_DEG) 下
//! 边缘 lean ≈ 5°，肉眼难察觉；且和 body lean 方向一致，反而强化
//! "sprite 是一个 3D 物体"的认知。
//!
//! # 父的 world rotation / position 怎么拿
//!
//! Bevy 的 [`Transform`] 是**局部坐标**。Y 轴 billboard 要拿父在世界里
//! 的位置（算从 sprite 指向 camera 的水平方向 → yaw）和父的世界旋转
//! （反算 sprite 该写多少 local rotation 才能合成目标 world rotation）。
//!
//! 本 system **直接通过 [`ChildOf`] 查父 entity 的 [`GlobalTransform`]**，
//! 一次查询同时读出干净的 `parent.world_rotation` 和
//! `parent.world_translation`。代价是每个 sprite 多一次 entity 查询，
//! 量级可忽略。
//!
//! 不假设父 rotation 永远 identity —— 容易踩的坑：挂 sprite 的 unit 一般
//! 带 [`avian3d::prelude::LockedAxes::ROTATION_LOCKED`] 防止被撞翻滚，
//! **但 Avian 0.6 这个 flag 只在每物理步把角速度三个分量清零，不会把
//! `Transform.rotation` 强制扣回 identity**。碰撞冲量、穿透推回、数值
//! 漂移都会给父 entity 加一点点旋转。这里直接读父当前 world rotation，
//! 漂移多少都会被自动补偿。
//!
//! # 为什么不"反推父 world"省一次 query
//!
//! 曾经写过 `parent.world_rotation = sprite.world_rotation ×
//! sprite.local_rotation⁻¹` 的版本，少一次 query 看似聪明。但这个等式
//! **只在 sprite local 是纯旋转时成立**。一旦 sprite local 里掺了非
//! 旋转分量（典型场景：玩家朝向左时给 sprite 子 entity 设
//! `Transform.scale.x = -1` 镜像翻转贴图，见
//! `unit::player::animation::tick_player_animation`），sprite 的 affine
//! 矩阵带反射（行列式 = -1）。glam 的
//! [`Affine3A::to_scale_rotation_translation`](bevy::math::Affine3A::to_scale_rotation_translation)
//! 会把 -1 塞到 `scale.x` 字段，剩下的"rotation"是从一组**左手系**正交
//! 基跑 Shepherd 法得到的伪四元数（不是 proper rotation）。这个 garbage
//! quat 代回反推公式，sprite world 累积出 scale / shear —— 相机一转，
//! sprite 疯狂闪烁扭曲。直接查父就规避了整套陷阱：父（unit entity）
//! 自身没有 scale 翻转，`GlobalTransform::rotation()` 返回的就是干净的
//! 旋转。

use bevy::prelude::*;

/// 项目统一像素密度：**32 像素 = 1 米**。
///
/// 见 [`doc/units-and-assets.md`](../../doc/units-and-assets.md) ——
/// 整个项目的美术资源都按这个尺子选 / 验收。
///
/// 用例：
/// ```ignore
/// let sprite_meters = pixel_height / PIXELS_PER_METER;
/// ```
pub const PIXELS_PER_METER: f32 = 32.0;

/// 标记一个 entity 是 billboard sprite，每帧由 [`face_camera`] 把 yaw
/// 对齐到相机方向（Y 轴 billboard，见模块顶部 doc）。
///
/// 这个组件本身**不渲染任何东西** —— 通常跟 [`Mesh3d`] + [`MeshMaterial3d`]
/// 一起挂在同一 entity 上：mesh 用 [`Rectangle::new(w, h)`] 当贴片（默认
/// 在 XY 平面、正面朝 +Z），material 用 [`StandardMaterial`]，关键参数：
///
/// - `base_color_texture`: PNG 贴图
/// - `alpha_mode: AlphaMode::Mask(0.5)`：抠图边缘干脆（`Blend` 会有半透
///   排序坑）
/// - `unlit: true`：不让 3D 光照"加工"手绘色，保留贴图原貌
/// - `cull_mode: None`：双面渲染，绕到背面也看得见
#[derive(Component, Debug, Default)]
pub struct BillboardSprite;

/// Billboard 插件 —— 注册每帧让所有 [`BillboardSprite`] 朝向相机的 system。
pub struct BillboardPlugin;

impl Plugin for BillboardPlugin {
    fn build(&self, app: &mut App) {
        // 放 PostUpdate：让 sprite 在 Update 阶段所有改 Transform 的逻辑
        // system 之后再朝向相机，避免被同帧其他系统覆盖。
        app.add_systems(PostUpdate, face_camera);
    }
}

/// 每帧把所有 [`BillboardSprite`] 的 yaw 对齐到相机方向（Y 轴 billboard）。
///
/// # 算法
///
/// 目标 **world rotation** = `Quat::from_rotation_y(yaw)`，其中 yaw 是
/// 在 XZ 平面上、从 sprite（用父的世界 XZ）指向相机的水平方向：
///
/// - sprite 局部 +Z → 相机方向（投影到 XZ 平面后）→ sprite 贴图正面对相机
/// - sprite 局部 +Y → world +Y → sprite 永远世界垂直（跟 body collider 同坐标系）
/// - sprite 局部 +X → yaw 旋转后的水平方向，配合 `scale.x = ±1` 镜像翻转
///   仍然把屏幕左右映射到角色左右（见 `tick_player_animation`）
///
/// 反算 local rotation：
///
/// ```text
/// yaw                   = atan2(camera.x - parent.x, camera.z - parent.z)
/// sprite.local_rotation = parent.world_rotation⁻¹ × Quat::from_rotation_y(yaw)
/// ```
///
/// `atan2(x, z)` 用 sprite 的世界 X / Z 偏差算"绕 Y 从 +Z 转到该方向"的
/// 角度。相机正在 sprite 正上方时 dx ≈ dz ≈ 0，`f32::atan2` 在 (0, 0)
/// 返回 0，sprite 退化为朝 world +Z，不需要特判。
///
/// 通过 [`ChildOf`] 查父 entity 的 [`GlobalTransform`]，一次拿到
/// `parent.world_rotation` 和 `parent.world_translation`。详见模块顶部
/// "父的 world rotation / position 怎么拿"。
///
/// # 假设
///
/// - **单相机**：场景里没有 [`Camera3d`] 或有多个则整帧静默跳过；多相机
///   分屏那天再扩展。
/// - **Rectangle mesh 正面是 +Z**：sprite 的正面贴图朝向局部 +Z 轴。
/// - **billboard 有父**：无父时退化到 identity 父旋转 / 原点位置。本
///   项目所有 sprite 都是 unit 的子，命中此分支等同误用。
/// - **sprite 跟父 XZ 重合**：项目里 sprite 的 LocalTransform 只在 Y 上
///   有 offset（脚部锚点抬高），XZ 都是 0；所以用父的 XZ 算 yaw = 用
///   sprite 自己 XZ 算 yaw。如果以后需要 sprite 在 XZ 上偏离父（武器
///   挂点等），这里要换成 sprite 自己的 GlobalTransform。
fn face_camera(
    cameras: Query<&GlobalTransform, (With<Camera3d>, Without<BillboardSprite>)>,
    mut sprites: Query<(&mut Transform, Option<&ChildOf>), With<BillboardSprite>>,
    parents: Query<&GlobalTransform, Without<BillboardSprite>>,
) {
    let Ok(camera_xform) = cameras.single() else {
        return;
    };
    let camera_pos = camera_xform.translation();

    for (mut transform, child_of) in &mut sprites {
        // 一次查询拿父的 world rotation + world translation。
        // 父没 scale 翻转，rotation() / translation() 都干净。
        // 没父时退化到 identity / 原点（误用兜底，本项目正常路径不命中）。
        let (parent_world_rot, parent_world_pos) = child_of
            .and_then(|c| parents.get(c.parent()).ok())
            .map(|gt| (gt.rotation(), gt.translation()))
            .unwrap_or((Quat::IDENTITY, Vec3::ZERO));

        // XZ 平面上算从 sprite 指向相机的方向，反推 yaw。
        // sprite 的 +Z 应指向相机：atan2(x, z) 给的就是绕 Y 从 +Z 转到
        // 该方向的角度。dx = dz = 0（相机正在 sprite 正上方）时 atan2
        // 返回 0，无需特判。
        let dx = camera_pos.x - parent_world_pos.x;
        let dz = camera_pos.z - parent_world_pos.z;
        let yaw = dx.atan2(dz);
        let desired_world_rot = Quat::from_rotation_y(yaw);

        transform.rotation = parent_world_rot.inverse() * desired_world_rot;
    }
}
