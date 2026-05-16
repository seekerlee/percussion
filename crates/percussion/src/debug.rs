//! Debug 可视化插件。
//!
//! 整个文件仅在 `debug_assertions` 开启（即非 `--release` / 非 `--profile dist`
//! 构建）时编译。发布构建里 `lib.rs` 用 `#[cfg(debug_assertions)] mod debug;`
//! 把这个模块整段跳过，零运行时开销。
//!
//! 当前包含：
//! - 原点坐标轴（红色 X / 绿色 Y）+ 灰色辅助网格（Gizmos 每帧重绘）
//! - 给每个 `Player` / `Monster` 头顶挂一个动态坐标标签

// Bevy 的 Query 类型本身就长，type_complexity 这条 lint 在 Bevy 项目里通常被
// 整体放行。范围仅限本文件，不影响业务模块。
#![allow(clippy::type_complexity)]

use crate::{Monster, Player};
use bevy::prelude::*;

/// Debug 可视化插件。挂上后启用网格、坐标轴、实体坐标标签。
pub struct DebugOverlayPlugin;

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (attach_position_labels, update_position_labels, draw_grid),
        );
    }
}

/// 跟随某个目标实体显示其世界坐标的浮动标签。
#[derive(Component)]
struct PositionLabel(Entity);

/// 给新出现的 Player / Monster 各生成一个独立标签实体，避免父子关系带来的
/// query 复杂度。标签自己持有目标实体 ID，每帧自行同步位置和文本。
fn attach_position_labels(
    mut commands: Commands,
    targets: Query<Entity, Or<(Added<Player>, Added<Monster>)>>,
) {
    for entity in &targets {
        commands.spawn((
            PositionLabel(entity),
            Text2d::new(""),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(1.0, 1.0, 1.0)),
            Transform::default(),
        ));
    }
}

/// 每帧：读目标实体 Transform → 写标签文本 + 把标签摆到目标头顶。
/// 目标若已 despawn 则静默跳过（标签会留下，原型阶段不必清理）。
fn update_position_labels(
    targets: Query<&Transform, Without<PositionLabel>>,
    mut labels: Query<(&PositionLabel, &mut Transform, &mut Text2d)>,
) {
    for (label, mut label_tf, mut text) in &mut labels {
        let Ok(target_tf) = targets.get(label.0) else {
            continue;
        };
        let pos = target_tf.translation.truncate();
        text.0 = format!("({:.0}, {:.0})", pos.x, pos.y);
        label_tf.translation = (pos + Vec2::new(0.0, 28.0)).extend(1.0);
    }
}

/// 用 Gizmos 画原点坐标轴 + 灰色辅助网格，每帧重绘。
fn draw_grid(mut gizmos: Gizmos) {
    const HALF_EXTENT: f32 = 600.0;
    const CELL: f32 = 100.0;

    let grid = Color::srgb(0.25, 0.25, 0.28);
    let axis_x = Color::srgb(1.0, 0.3, 0.3);
    let axis_y = Color::srgb(0.3, 1.0, 0.3);

    // 网格：每 CELL 单位一条灰线
    let cells = (HALF_EXTENT / CELL) as i32;
    for i in -cells..=cells {
        if i == 0 {
            continue; // 0 这条留给主轴单独画
        }
        let v = i as f32 * CELL;
        gizmos.line_2d(Vec2::new(v, -HALF_EXTENT), Vec2::new(v, HALF_EXTENT), grid);
        gizmos.line_2d(Vec2::new(-HALF_EXTENT, v), Vec2::new(HALF_EXTENT, v), grid);
    }

    // 主轴：X 红 / Y 绿
    gizmos.line_2d(
        Vec2::new(-HALF_EXTENT, 0.0),
        Vec2::new(HALF_EXTENT, 0.0),
        axis_x,
    );
    gizmos.line_2d(
        Vec2::new(0.0, -HALF_EXTENT),
        Vec2::new(0.0, HALF_EXTENT),
        axis_y,
    );
}
