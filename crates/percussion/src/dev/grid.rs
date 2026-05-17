//! XZ 地面网格 + 原点三轴可视化。
//!
//! 本模块由 `crate::dev` 在 `#[cfg(debug_assertions)]` 下统一编译。发布
//! 构建里整个 `dev` 模块都不存在，零运行时开销。
//!
//! 当前包含：
//! - XZ 地面平面上的灰色网格（每 `GRID_STEP` 单位一条线）
//! - 原点三轴（X 红 / Y 绿 / Z 蓝）—— Bevy 3D 标准配色
//!
//! 视觉技术路线见 `doc/game-design.md` §15（3D 世界 + 全 2D sprite + Y 轴
//! billboard）。网格画在 XZ 平面 Y=0，配合 lib.rs 的俯视斜角相机。
//!
//! # 关于"动态视野范围"
//!
//! 早期 2D 版本用 `OrthographicProjection.area` 算出"当前可见矩形"动态裁
//! 网格。3D 透视相机的可见地面是一个**梯形**（视锥与 Y=0 平面相交后的四边
//! 形，且边平行于相机视锥而不是世界轴），算起来更复杂；原型期先用以原点
//! 为中心 ±`GRID_EXTENT` 的固定方形网格，等真正需要"无限网格"再换 shader
//! 方式（地面 grid shader 是 3D 项目的标准做法）。

use bevy::prelude::*;

/// 网格间距：1 单位（约 1 米）一条线。
const GRID_STEP: f32 = 1.0;
/// 网格半边长：以原点为中心 ±`GRID_EXTENT` 的方形覆盖。
const GRID_EXTENT: f32 = 20.0;

/// 网格 + 三轴可视化插件。
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw_grid);
    }
}

/// 用 Gizmos 在 XZ 平面（Y=0）画网格 + 原点三轴。每帧重绘。
fn draw_grid(mut gizmos: Gizmos) {
    let grid = Color::srgb(0.25, 0.25, 0.28);
    let axis_x = Color::srgb(1.0, 0.3, 0.3);
    let axis_y = Color::srgb(0.3, 1.0, 0.3);
    let axis_z = Color::srgb(0.3, 0.5, 1.0);

    let steps = (GRID_EXTENT / GRID_STEP) as i32;

    // XZ 平面网格：跳过 0（那两条由主轴接管，颜色不同）。
    for i in -steps..=steps {
        if i == 0 {
            continue;
        }
        let coord = i as f32 * GRID_STEP;
        // 平行于 X 轴的线（z 固定）
        gizmos.line(
            Vec3::new(-GRID_EXTENT, 0.0, coord),
            Vec3::new(GRID_EXTENT, 0.0, coord),
            grid,
        );
        // 平行于 Z 轴的线（x 固定）
        gizmos.line(
            Vec3::new(coord, 0.0, -GRID_EXTENT),
            Vec3::new(coord, 0.0, GRID_EXTENT),
            grid,
        );
    }

    // 三轴：X 红、Z 蓝（贴地），Y 绿（穿过原点上下贯通 ±GRID_EXTENT）。
    // 三轴长度一致，方便目视判断空间方向。
    gizmos.line(
        Vec3::new(-GRID_EXTENT, 0.0, 0.0),
        Vec3::new(GRID_EXTENT, 0.0, 0.0),
        axis_x,
    );
    gizmos.line(
        Vec3::new(0.0, 0.0, -GRID_EXTENT),
        Vec3::new(0.0, 0.0, GRID_EXTENT),
        axis_z,
    );
    gizmos.line(
        Vec3::new(0.0, -GRID_EXTENT, 0.0),
        Vec3::new(0.0, GRID_EXTENT, 0.0),
        axis_y,
    );
}
