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
//! # 用 full billboard（不是 Y 轴 billboard）
//!
//! 「**Y 轴 billboard**」只把 sprite 在水平面 yaw 转向相机，sprite 在 world
//! 里始终垂直于地面。问题：透视相机 + 斜俯角下，**世界 +Y 不在 image
//! plane 里**（因为相机有 pitch），所以"世界垂直"的 sprite 投影到屏幕
//! 上时，离屏幕中心越远，越往中心顶部消失点倾斜 —— 视觉上"人没垂直
//! 于地面"。这是透视投影的固有几何，sprite 本身没歪。
//!
//! 「**Full billboard**」让 sprite 的 plane 跟相机 image plane 完全平行：
//! sprite world rotation **直接等于相机 world rotation**。等价于 sprite
//! 局部三轴跟相机三轴对齐 —— 局部 +Z 朝相机、局部 +Y 朝相机的 image-up
//! 方向、局部 +X 朝相机的 image-right。
//!
//! 屏幕上 sprite 永远是正立矩形（头朝屏幕正上方）。代价：sprite 在 world
//! 里**斜着躺**，匹配相机 pitch + yaw —— 视觉上"脚"会偏离 sprite 局部
//! 中心连线对应的 world 地面点（详见 [`face_camera`] 文档）。这个项目目前
//! 不画 sprite 自身阴影、相机也不会做大幅 pitch 变化，偏移不显眼。
//!
//! # 父的 world rotation 怎么拿
//!
//! Bevy 的 [`Transform`] 是**局部坐标**，sprite 的 world rotation =
//! parent.world_rotation × sprite.local_rotation。billboard 想让 sprite
//! world rotation 等于相机 world rotation，必须先知道父的 world rotation
//! 才能反算 local rotation 该写多少。
//!
//! 本 system **直接通过 [`ChildOf`] 查父 entity 的 [`GlobalTransform`]**，
//! 读出干净的 `parent.world_rotation`。代价是每个 sprite 多一次 entity
//! 查询，量级可忽略。
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

/// 标记一个 entity 是 billboard sprite，每帧由 [`face_camera`] 把姿态
/// 对齐到相机的 image plane（full billboard，见模块顶部 doc）。
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

/// 每帧把所有 [`BillboardSprite`] 的姿态对齐到相机 image plane（full
/// billboard）。
///
/// # 算法
///
/// 目标 **world rotation** 直接等于相机的 world rotation —— sprite 局部
/// 三轴跟相机三轴一一对齐：
///
/// - sprite 局部 +Z → 相机 +Z（朝相机背后方向）→ sprite 正面贴图对着相机
/// - sprite 局部 +Y → 相机 +Y（image-up）→ sprite 的"头"投影成屏幕正上
/// - sprite 局部 +X → 相机 +X（image-right）
///
/// 反算 local rotation：
///
/// ```text
/// sprite.local_rotation = parent.world_rotation⁻¹ × camera.world_rotation
/// ```
///
/// 通过 [`ChildOf`] 查父 entity 的 [`GlobalTransform`] 拿
/// `parent.world_rotation`。详见模块顶部"父的 world rotation 怎么拿"。
///
/// # 已知视觉代价："脚"漂移
///
/// sprite 在 world 里斜着躺（匹配相机 pitch + yaw），所以 sprite 局部
/// (0, -h/2, 0) 这个视觉"脚"点在 world 里 = sprite_center - (h/2) × camera_up。
/// 跟 sprite_center 正下方的地面点不重合 —— 视觉上"脚"会比物理 collider
/// 位置往相机方向偏 sin(pitch) × (h/2)、垂直方向沉 cos(pitch) × (h/2) - h/2。
///
/// 当前不画 sprite 自身阴影、release 相机 pitch 固定 = 45°，偏移肉眼
/// 不太察觉。如果以后要做"脚踩地面特效"或自身阴影对齐，再加一层 local
/// translation 补偿（每帧按 camera_up 反向平移 h/2）。
///
/// # 假设
///
/// - **单相机**：场景里没有 [`Camera3d`] 或有多个则整帧静默跳过；多相机
///   分屏那天再扩展。
/// - **Rectangle mesh 正面是 +Z**：sprite 的正面贴图朝向局部 +Z 轴。
/// - **billboard 有父**：无父时退化到 identity 父旋转。本项目所有 sprite
///   都是 unit 的子，命中此分支等同误用。
fn face_camera(
    cameras: Query<&GlobalTransform, (With<Camera3d>, Without<BillboardSprite>)>,
    mut sprites: Query<(&mut Transform, Option<&ChildOf>), With<BillboardSprite>>,
    parents: Query<&GlobalTransform, Without<BillboardSprite>>,
) {
    let Ok(camera_xform) = cameras.single() else {
        return;
    };

    // 目标 world rotation：sprite 三轴 = 相机三轴。
    // 拿一次就够 —— 所有 sprite 共用同一个目标。
    let desired_world_rot = camera_xform.rotation();

    for (mut transform, child_of) in &mut sprites {
        // 直接读父的 world rotation。父没 scale 翻转，rotation() 干净。
        // 没父时退化到 identity（误用兜底，本项目正常路径不命中）。
        let parent_world_rot = child_of
            .and_then(|c| parents.get(c.parent()).ok())
            .map(|gt| gt.rotation())
            .unwrap_or(Quat::IDENTITY);

        transform.rotation = parent_world_rot.inverse() * desired_world_rot;
    }
}
