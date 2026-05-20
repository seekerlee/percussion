//! 把玩家的 4 张分动作 sprite sheet 横向拼成单张 `sheet.png`。
//!
//! # 为什么需要
//!
//! `bevy_sprite3d` 在 entity spawn 时把每帧 UV 烤进 mesh 缓存，**整张
//! 图的尺寸**参与 UV 计算（`frac_rect = URect / image_size`）。所以一
//! 个 sprite entity 只能绑定一张图；想要 idle / run / attack / jump
//! 用同一个 entity 切换，必须先把 4 张图拼成一张，再用一份共享的
//! `TextureAtlasLayout` 描述各动作占用的 frame index range。
//!
//! # 输入 / 输出
//!
//! 输入：`crates/percussion/assets/sprites/units/player/{idle,run,attack,jump}.png`，
//! 每张都是 128×64 单帧横向排列（高 = 64，宽 = N × 128）。
//!
//! 输出：同目录下的 `sheet.png`，宽 = 所有源图宽度之和，高 = 64。
//! 各动作在 sheet 上的起始 frame index 印到 stdout，供游戏代码硬编
//! 码引用（[`crates/percussion/src/unit/player/animation.rs`] 之类）。
//!
//! # 跑法
//!
//! ```text
//! cargo run -p tools --bin stitch_player_sprites
//! ```
//!
//! 加 / 改动作时：换源 png、改下面的 `ACTIONS` 列表、重跑、把打印
//! 出的 range 同步进游戏代码。

use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use image::{GenericImage, RgbaImage};

/// 单帧像素宽度。
const FRAME_WIDTH: u32 = 128;
/// 单帧像素高度。
const FRAME_HEIGHT: u32 = 64;

/// 待合并的动作列表。
///
/// 顺序 = 在 sheet 上从左到右的排列顺序 = `TextureAtlasLayout` 的 frame
/// index 顺序。**改这个列表前先想清楚**：删除 / 重排会让游戏代码里
/// 已有的 index range 全部失效。新增 append 到末尾最安全。
const ACTIONS: &[(&str, &str)] = &[
    ("idle", "idle.png"),
    ("run", "run.png"),
    ("attack", "attack.png"),
    ("jump", "jump.png"),
];

/// 输出 sheet 的文件名（位于源图同目录）。
const OUTPUT_NAME: &str = "sheet.png";

fn main() -> Result<()> {
    // CARGO_MANIFEST_DIR 在 `cargo run` 时指向本 crate 根（crates/tools/）。
    // 用它做锚点定位主游戏的 assets 目录，避免依赖当前工作目录。
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let asset_dir = manifest_dir
        .parent() // crates/
        .context("tools crate 应位于 crates/ 下")?
        .join("percussion")
        .join("assets")
        .join("sprites")
        .join("units")
        .join("player");

    ensure!(
        asset_dir.is_dir(),
        "找不到资产目录：{}",
        asset_dir.display()
    );

    // 一次性把所有源图读进内存。规模小（每张 128×64×N，几十 KB），
    // 不做流式处理；读完再开始拼，任一张坏掉都还来得及报错退出。
    let sources: Vec<(&str, RgbaImage)> = ACTIONS
        .iter()
        .map(|(name, file)| -> Result<_> {
            let path = asset_dir.join(file);
            let img = image::open(&path)
                .with_context(|| format!("打开 {} 失败", path.display()))?
                .to_rgba8();
            ensure!(
                img.height() == FRAME_HEIGHT,
                "{} 高度 {} ≠ 期望 {}",
                file,
                img.height(),
                FRAME_HEIGHT
            );
            ensure!(
                img.width() % FRAME_WIDTH == 0,
                "{} 宽度 {} 不是 {} 的整数倍",
                file,
                img.width(),
                FRAME_WIDTH
            );
            Ok((*name, img))
        })
        .collect::<Result<_>>()?;

    let total_width: u32 = sources.iter().map(|(_, img)| img.width()).sum();
    let mut sheet = RgbaImage::new(total_width, FRAME_HEIGHT);

    // 逐张 copy_from 而不是 imageops::overlay：前者是直接像素覆写、
    // 后者会做 alpha 混合。源图本身已经是透明背景 + 不透明人物，画
    // 在全透明 canvas 上两种行为结果一样，但 copy_from 语义更直白
    // ——"我们就是在拼贴，不是在合成"。
    let mut x_offset = 0u32;
    let mut frame_cursor = 0u32;
    println!("== Sheet layout ==");
    for (name, src) in &sources {
        sheet
            .copy_from(src, x_offset, 0)
            .with_context(|| format!("拷贝 {} 到 sheet 失败", name))?;
        let frame_count = src.width() / FRAME_WIDTH;
        println!(
            "  {:<8} {:>2} frames  range {}..{}",
            name,
            frame_count,
            frame_cursor,
            frame_cursor + frame_count
        );
        x_offset += src.width();
        frame_cursor += frame_count;
    }

    let out_path = asset_dir.join(OUTPUT_NAME);
    sheet
        .save(&out_path)
        .with_context(|| format!("写入 {} 失败", out_path.display()))?;

    println!(
        "== Wrote {} ({}×{}, {} frames total) ==",
        out_path.display(),
        total_width,
        FRAME_HEIGHT,
        frame_cursor
    );

    Ok(())
}
