//! 开发时 egui inspector —— 编辑器风格的实体 / 组件可视化与即时编辑。
//!
//! 本模块由 `crate::dev` 在 `#[cfg(debug_assertions)]` 下统一编译；release
//! 构建里整个 `dev` 模块不存在，零运行时开销、零二进制体积。底层依赖
//! `bevy-inspector-egui` 也只通过这条 `cfg` 路径被引用，发布构建里 LTO
//! 会把它一起剥掉。
//!
//! # 它解决什么问题
//!
//! 开发期想"边玩边调相机 / 灯光 / 物理参数"——常规做法是改常量重编。
//! 装上这个面板后：左侧弹出一棵 World 树，所有 `Reflect` 的组件 / 资源
//! 直接以滑块、数字输入、Vec3 输入等控件展开，运行时改的值立刻生效。
//! 找到舒服的视角 / 灯光后，再把数值抄回 `lib.rs` 的常量。
//!
//! # 心智模型
//!
//! 跟 `dev::camera`（pan-orbit 控制器）正交：camera 模块负责**怎么操控**
//! 相机，inspector 模块负责**显示与编辑**任意组件的字段。两者都作用在
//! 同一个 Camera3d entity 上，互不耦合。
//!
//! # 为什么 `register_type::<PanOrbitCamera>()`
//!
//! `bevy_panorbit_camera` 0.34 给 `PanOrbitCamera` 加了 `#[derive(Reflect)]`
//! 和 `#[reflect(Component)]`，但插件自己**没**调 `register_type`。
//! Bevy 的 reflect 注册表是运行时构造的，没注册的类型 inspector 不知道
//! 怎么展开字段——只能看到一个名字、点不开。补这一行让 PanOrbitCamera
//! 的字段（`target_yaw`、`target_pitch`、`target_radius`、`target_focus`）
//! 都能在面板里直接拖动。
//!
//! Bevy 自带的 `Transform`、`Projection` 等类型由 Bevy 各 plugin 自行
//! `register_type`，不需要我们再注册。

use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_panorbit_camera::PanOrbitCamera;

/// World inspector 插件。仅 debug 构建里被 [`GamePlugin`](crate::GamePlugin) 注册。
pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .add_plugins(WorldInspectorPlugin::new())
            // 第三方 reflectable 组件，作者没在自己 plugin 里注册。
            // 这里补一刀，让 inspector 能展开它的字段。
            .register_type::<PanOrbitCamera>();
    }
}
