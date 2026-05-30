//! Forest props（树木、灌木等）—— stage 上的静态装饰物，第一个走
//! aseprite 资源路径的领域。
//!
//! # 为什么单独成模块
//!
//! 树 / 灌木 / 落叶等概念上是某种 stage 的"场景内容"，跟 stage 几何
//! （地面、墙、屏障）平级。stage.rs 只管骨架（边界 + 屏障），具体放
//! 什么 props 由各个 prop 类型自己管 —— 类似 [`crate::unit::dragon1`]
//! 跟 [`crate::stage`] 的关系：stage 提供舞台，单位 / 装饰由独立模块
//! 各自定义资产 + spawn API。
//!
//! 这套模式让"以后所有新 sprite 资源都用 aseprite"变成机械操作：
//! 每个新领域加一个自家的 `XxxAssets` + `spawn_xxx` 即可，跟 stage /
//! 其他领域零耦合。
//!
//! # bevy_aseprite_ultra 在这里的角色
//!
//! **只当 aseprite 二进制文件的解析器**，不用它自带的 `AseSlice` /
//! `AseAnimation` 组件。原因：ultra 的 3D 渲染路径要求用户自定义
//! [`bevy::pbr::Material`]，跟项目现有的 `bevy_sprite3d` +
//! `StandardMaterial` 链路冲突。
//!
//! 好在 ultra 的 [`Aseprite`] asset 字段是 public 的 —— 加载完
//! `.aseprite` 后内部已经构建了一张 atlas image + [`TextureAtlasLayout`]，
//! 跟项目现有 PNG sheet 路径用的数据结构完全同构。我们 spawn 时直接
//! 从 [`Aseprite::atlas_image`] / [`Aseprite::atlas_layout`] / 对应
//! slice 的 [`SliceMeta::atlas_id`] 拿值，喂给标准的
//! [`Sprite::from_atlas_image`] + [`Sprite3d`] —— 跟 [`spawn_dragon1`]
//! 路径长得完全一样。
//!
//! 代价：失去 ultra 的 hot reload（它的 hot reload 依赖 `AseSlice` /
//! `AseAnimation` 组件触发重渲）。改图必须重启游戏。第一阶段验证
//! plugin 能跑就行，hot reload 以后再说。
//!
//! [`spawn_dragon1`]: crate::unit::dragon1::spawn_dragon1

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use bevy_asset_loader::prelude::*;
use bevy_sprite3d::prelude::*;

use crate::app_state::AppState;
use crate::sprite_billboard::{BillboardSprite, PIXELS_PER_METER};

/// Forest 领域的预加载资产集合。
///
/// 行为完全等价于 [`Dragon1Assets`](crate::unit::dragon1::Dragon1Assets) /
/// [`PlayerAssets`](crate::unit::player::PlayerAssets)：
/// [`bevy_asset_loader`] 在 [`AppState::Loading`] 阶段把 aseprite 加载好，
/// 整个 collection 作为 [`Resource`] insert，进入 [`AppState::InGame`] 后
/// `Res<ForestAssets>` 拿到的 handle 保证就绪。
///
/// `.aseprite` 文件已经在 loader 里以 nearest sampler 加载（见
/// [`AsepriteLoaderSettings::default`]），不需要像 PNG 那样显式
/// `#[asset(image(sampler(...)))]`。
#[derive(AssetCollection, Resource)]
pub struct ForestAssets {
    /// 树木 spritesheet —— 多种树 / 草作为 named slice 存在同一个文件里
    /// （`tree1` / `tree2` / `tree3` / `tree4` / `tree5` / `tree6` /
    /// `deadtree` / `grass1left` / `grass1middle` / `grass1right` /
    /// `grass2left` / `grass2middle` / `grass2right` / `deadtreeground`）。
    /// slice 名 → 取哪一块由 spawn 时的字符串参数决定，见 [`spawn_tree`]。
    ///
    /// 缺文件会让 LoadingState 永不就绪、游戏卡在 Loading 黑屏 —— 跟其他
    /// `AssetCollection` 的硬依赖语义一致。
    #[asset(path = "sprites/props/forest/Trees_Alt.aseprite")]
    pub trees: Handle<Aseprite>,
}

/// Forest 插件 —— 触发 [`ForestAssets`] 在 Loading state 加载。
///
/// 必须在 [`AppStatePlugin`](crate::app_state::AppStatePlugin) 之后 add，
/// 否则 `LoadingState` 还没注册会 panic。
pub struct ForestPlugin;

impl Plugin for ForestPlugin {
    fn build(&self, app: &mut App) {
        app.configure_loading_state(
            LoadingStateConfig::new(AppState::Loading).load_collection::<ForestAssets>(),
        );
    }
}

/// 在指定 stage 下 spawn 一棵 / 一丛由 `slice_name` 指定的 forest prop，返回 entity。
///
/// # 结构
///
/// 不像 [`spawn_dragon1`](crate::unit::dragon1::spawn_dragon1) 分父 + sprite 子
/// 两层 —— 树木目前没有物理 body、不需要镜像翻转，**单 entity 就够**：
/// 同一个 entity 上挂 `Sprite3d` + `Sprite` + [`BillboardSprite`] +
/// [`Transform`] + `ChildOf(parent_stage)`。需要加 collider / AI 时再拆分。
///
/// # Pivot 与位置
///
/// `Sprite3d::pivot = (0.5, 0.0)` 让贴图"脚中"对齐 sprite mesh 局部
/// 原点 —— 调用方传的 `local_pos.y = 0` 就意味着"树脚刚好踩在地面"。
/// 跟 [`spawn_dragon1`] / [`spawn_player`](crate::unit::player::spawn_player)
/// 的子 sprite Y 偏移逻辑同源，只是这里直接合并到 entity 自身。
///
/// # 错误处理
///
/// `slice_name` 拼错或在 aseprite 文件里不存在 → panic。slice 名是开
/// 发期常量字符串，运行时拼错就是 bug，越早炸越好。
pub fn spawn_tree(
    commands: &mut Commands,
    forest_assets: &ForestAssets,
    aseprites: &Assets<Aseprite>,
    slice_name: &str,
    parent_stage: Entity,
    local_pos: Vec3,
) -> Entity {
    let aseprite = aseprites
        .get(&forest_assets.trees)
        .expect("ForestAssets.trees must be ready in AppState::InGame");
    let slice = aseprite
        .slices
        .get(slice_name)
        .unwrap_or_else(|| panic!("slice `{slice_name}` not found in Trees_Alt.aseprite"));

    commands
        .spawn((
            BillboardSprite,
            Sprite3d {
                pixels_per_metre: PIXELS_PER_METER,
                unlit: true,
                pivot: Some(Vec2::new(0.5, 0.0)),
                ..default()
            },
            Sprite::from_atlas_image(
                aseprite.atlas_image.clone(),
                TextureAtlas {
                    layout: aseprite.atlas_layout.clone(),
                    index: slice.atlas_id,
                },
            ),
            Transform::from_translation(local_pos),
            ChildOf(parent_stage),
        ))
        .id()
}
