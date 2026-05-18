//! 开发时（debug 构建）可视化 + 操作辅助的集中入口。
//!
//! 整个模块通过 `lib.rs` 顶层的 `#[cfg(debug_assertions)] mod dev;` 只在
//! debug 构建里编译。release / `--profile dist` 构建里此目录下所有文件
//! 都不会被编译，零运行时开销、零二进制体积。
//!
//! # 包含哪些子模块
//!
//! - [`grid`]：XZ 地面网格 + 原点三轴的 gizmo 可视化
//! - [`camera`]：pan-orbit + WASD 相机控制器，把固定相机变得可拖动
//! - [`inspector`]：egui World inspector，运行时编辑任意 Reflect 组件
//!
//! # 加东西的约定
//!
//! 后续 dev 工具（FPS overlay、stage bounds 框、状态文字 …）也都放这里，
//! 每个工具一个文件，对外暴露一个 `*Plugin`。`lib.rs` 在 debug 构建里
//! 集中注册 `dev::*::Plugin`。
//!
//! 不要把 release 也需要的东西放进来 —— 那是 `lib.rs` / 各 gameplay 模块
//! 的责任。本模块的语义边界是"仅 dev 构建存在"。

pub mod camera;
pub mod grid;
pub mod inspector;
