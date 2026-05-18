//! 全局应用状态机 + 资产预加载阶段。
//!
//! # 这个模块解决什么问题
//!
//! Bevy 的 `asset_server.load()` 是异步的：handle 立刻返回，但图片真正
//! 解码 / 上 GPU 是后台线程几帧后才完成。如果在 `Startup` 里直接 spawn
//! 用到这些图的实体，会有三类隐患：
//!
//! 1. **第一帧空白**：material 引用的 texture 还没就绪，那帧渲染缺贴图；
//! 2. **`bevy_sprite3d` 类的"需要图片尺寸"插件**：它的内部 system 会因
//!    `Assets<Image>::get` 拿到 `None` 跳过当前 entity，行为隐式；
//! 3. **依赖隐式**：调用方靠"读注释"知道哪些 handle 必须等 —— 容易漏。
//!
//! 引入 [`AppState`] + [`bevy_asset_loader`] 的 LoadingState 之后，所有
//! 游戏内容 spawn 都搬到 [`AppState::InGame`] 的 `OnEnter` 上 —— 进入这
//! 个 state 时，所有标过 `AssetCollection` 的资源都**保证已加载完毕**，
//! spawn 路径里拿到 [`Res<XxxAssets>`] 就是拿到已就绪的 handle，类型层面
//! 阻挡"忘了等加载"。
//!
//! # 模块边界
//!
//! 这里**只管状态机本身**：定义 state 枚举、注册 LoadingState、声明加载
//! 完成后跳到哪个 state。**不知道**有哪些具体 asset、不 spawn 任何 entity。
//! 各领域 plugin（如 [`crate::player::PlayerPlugin`]）通过
//! [`bevy_asset_loader::loading_state::config::ConfigureLoadingState::configure_loading_state`]
//! 把自家 `AssetCollection` 挂到这个 LoadingState 上，实现"加载列表分布
//! 在各模块里、状态机集中"的职责分离。
//!
//! # 顺序约束
//!
//! [`AppStatePlugin`] **必须**在所有调用 `configure_loading_state` 的领
//! 域 plugin 之前 add —— 否则 LoadingState 还没注册，配置请求会 panic。

use bevy::prelude::*;
use bevy_asset_loader::prelude::*;

/// 全局应用状态机。
///
/// 现阶段只有"加载中 → 游戏中"两个 variant。后续添加 `MainMenu` /
/// `Paused` / `GameOver` 都是加 variant 的事，不影响现有状态之间的过渡
/// 配置 —— `LoadingState::continue_to_state` 仍然指向 `InGame`。
#[derive(States, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum AppState {
    /// 资产预加载阶段。App 启动时的**默认状态**（`#[default]`），由
    /// [`bevy_asset_loader`] 监控所有注册的 `AssetCollection` 是否就绪，
    /// 就绪后自动跳到 [`AppState::InGame`]。
    ///
    /// 这个阶段相机 / 全局光仍然存在（它们挂在 `Startup` 而非 `OnEnter`
    /// 上），所以窗口、debug 网格、未来的"Loading..." UI 都可以正常渲染。
    #[default]
    Loading,
    /// 游戏运行中。进入此状态时所有 `AssetCollection` 资源都保证已 insert
    /// 到 World，可以放心 spawn 游戏内容。
    InGame,
}

/// 注册 [`AppState`] 状态机 + bevy_asset_loader 的 LoadingState 配置。
///
/// 单一职责 —— 只搭框架，不知道任何具体 asset。各模块在自己的 `PluginPlugin`
/// 里用 [`bevy_asset_loader::loading_state::config::ConfigureLoadingState`]
/// trait 的 `configure_loading_state` 方法把 `AssetCollection` 挂进来。
pub struct AppStatePlugin;

impl Plugin for AppStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>().add_loading_state(
            LoadingState::new(AppState::Loading).continue_to_state(AppState::InGame),
        );
    }
}
