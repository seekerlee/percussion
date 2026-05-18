//! Y 轴 billboard sprite —— 让 2D 贴片在 3D 场景里始终面对相机。
//!
//! # 这个模块解决什么问题
//!
//! 项目的视觉路线是 **3D 世界 + 2D billboard sprite**（饥荒 / Delver /
//! The Last Night 都是这套）：相机以斜俯角看 3D 场景，但角色、道具、
//! 特效都是平面 2D 贴图，靠每帧把贴图的 yaw 转向相机来"伪装"成立体。
//!
//! 这个模块只管"朝向相机"这一件事 —— 不管贴图怎么挑、mesh 怎么建、
//! material 怎么配。调用方 spawn 一个 [`Mesh3d`] + [`MeshMaterial3d`]
//! 后挂上 [`BillboardSprite`] marker，剩下交给本模块的 system。
//!
//! 因为 billboard 是**渲染层**通用机制（道具、地面提示、伤害浮字以后
//! 也都会用），不属于 unit / player / stage 任一具体领域，所以独立成
//! 模块。
//!
//! # 为什么只绕 Y 轴
//!
//! 完整 billboard（同时绕 X 和 Y 跟随相机）在俯视斜角下会让 sprite
//! 看起来"飘"—— 相机有 pitch，sprite 也跟着 pitch，像树倒了一样。
//! 饥荒、Delver 都只绕 Y：sprite 保持垂直，只在水平面内转身追相机。
//!
//! # 对父旋转鲁棒
//!
//! Bevy 的 [`Transform`] 是**局部坐标**，sprite 的 world rotation =
//! parent.world_rotation × sprite.local_rotation。如果直接给 local 写
//! 一个纯 yaw（像旧版本那样调 [`Transform::look_to`]），只要父有任何
//! pitch / roll，sprite 在 world 里就跟着歪 —— 视觉表现为"人没垂直
//! 于地面"。
//!
//! 容易踩的坑：挂 sprite 的 unit 一般会带 [`avian3d::prelude::LockedAxes::ROTATION_LOCKED`]
//! 防止被撞翻滚，**但 Avian 0.6 的这个 flag 只在每物理步把角速度三个
//! 分量清零，不会把 `Transform.rotation` 强制扣回 identity**。碰撞冲量、
//! 穿透推回、数值漂移都会给父 entity 加一点点旋转，几秒后累积成肉眼
//! 可见的歪。
//!
//! 因此本 system 不假设父级旋转 —— 算出目标 **world rotation**（纯
//! Y 轴 yaw），再反推该写到 local rotation 的值（见 [`face_camera_yaw`]）。

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

/// 标记一个 entity 是 Y 轴 billboard sprite，每帧由 [`face_camera_yaw`]
/// 把 yaw 旋到对准相机。
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
        app.add_systems(PostUpdate, face_camera_yaw);
    }
}

/// 每帧把所有 [`BillboardSprite`] 的 yaw 旋到水平指向相机。
///
/// # 算法
///
/// 1. 算"sprite → 相机"的水平向量（抹掉 Y）；
/// 2. 用 [`f32::atan2`] 取这个向量在 XZ 平面上的角度，作为目标 yaw；
/// 3. 目标 **world rotation** = [`Quat::from_rotation_y`]（绝对纯 Y 轴旋转，
///    跟父级状态无关）；
/// 4. 反推 local rotation —— 见模块顶部"对父旋转鲁棒"。
///
/// # 反推 local rotation 的代数
///
/// 已知：
/// - `current_world = parent_world × current_local`（Bevy transform 传播）
/// - 想要：`new_world = parent_world × new_local`，且 `new_world = desired`
///
/// 解出：
/// - `parent_world = current_world × current_local⁻¹`
/// - `new_local = parent_world⁻¹ × desired = current_local × current_world⁻¹ × desired`
///
/// 用 sprite 自己上一帧的 [`GlobalTransform`] + 当前 [`Transform`] 就能
/// 反推出 parent world rotation，不必再 query 父 entity。落后一帧的代价
/// 是父旋转漂移那一丁点；俯视斜角下完全看不出来。
///
/// # 假设
///
/// - **单相机**：场景里没有 [`Camera3d`] 或有多个则整帧静默跳过；多相机
///   分屏那天再扩展。
/// - **Rectangle mesh 正面是 +Z**：sprite 的正面贴图朝向局部 +Z 轴。
fn face_camera_yaw(
    cameras: Query<&GlobalTransform, (With<Camera3d>, Without<BillboardSprite>)>,
    mut sprites: Query<(&mut Transform, &GlobalTransform), With<BillboardSprite>>,
) {
    let Ok(camera_xform) = cameras.single() else {
        return;
    };
    let cam_pos = camera_xform.translation();

    for (mut transform, sprite_global) in &mut sprites {
        let delta = cam_pos - sprite_global.translation();
        let flat_to_cam = Vec3::new(delta.x, 0.0, delta.z);

        if flat_to_cam.length_squared() < 1e-6 {
            // 相机几乎正好在 sprite 正上方，水平朝向退化为不定 —— 保留
            // 上一帧 rotation，避免突变。俯视斜角下不会真碰到这个分支。
            continue;
        }

        // 目标 world rotation：纯 Y 轴 yaw，让局部 +Z 在水平面内指向相机。
        // 绕 +Y 转 θ 后：+Z = (0,0,1) → (sin θ, 0, cos θ)，匹配 flat_to_cam 方向
        // 要求 sin θ = flat.x / |flat|、cos θ = flat.z / |flat| → θ = atan2(flat.x, flat.z)。
        let yaw = flat_to_cam.x.atan2(flat_to_cam.z);
        let desired_world_rot = Quat::from_rotation_y(yaw);

        // 反推 local rotation（见 doc 注释里的代数推导）。
        // 注意：sprite_global 是上一帧 transform 传播的产物，与 transform.rotation
        // 当前值组合即可还原父 world rotation；这正是我们需要的。
        transform.rotation =
            transform.rotation * sprite_global.rotation().inverse() * desired_world_rot;
    }
}
