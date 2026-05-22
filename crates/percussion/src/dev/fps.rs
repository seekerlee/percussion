//! Dev 构建里在窗口左上角显示 FPS / 帧时间 + 帧时间曲线图。
//!
//! 用途是**诊断**：肉眼看 FPS 数字 + 一条滚动的 frame time graph，能马上
//! 分辨"持续慢"（条形整体高）/ "周期 spike"（条形里规律性的尖刺）/
//! "随机 spike"（无规律的高条）三种掉帧模式。`bevy_dev_tools` 的
//! `FpsOverlayPlugin` 自带 graph，零额外代码。
//!
//! 整模块通过 `dev` 父模块的 `#[cfg(debug_assertions)]` 守卫，
//! release / dist 构建不编译、零运行时开销。

use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin, FrameTimeGraphConfig};
use bevy::prelude::*;

/// 接入 Bevy 自带的 FPS overlay 插件，参数针对本项目的诊断需求微调。
pub struct FpsPlugin;

impl Plugin for FpsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FpsOverlayPlugin {
            config: FpsOverlayConfig {
                // 默认 text + graph 都开。graph 的 min/target 决定柱状条的
                // 颜色映射：低于 min 红色、高于 target 绿色、中间渐变。
                // 60fps 是 vsync 上限的常见值，30 是"明显卡"的阈值。
                frame_time_graph_config: FrameTimeGraphConfig {
                    enabled: true,
                    min_fps: 30.0,
                    target_fps: 60.0,
                },
                ..default()
            },
        });
    }
}
