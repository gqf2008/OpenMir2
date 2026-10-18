//! 玩家 `PlayObject.Run` **周期缺口段（run-gap）** 的取数站点。
//!
//! 本模块只放「必须独立成 trace 站点组」的取数入口（口径照 `magic_effect.rs` 的迁出理由）：
//! `consumer_groups.tsv` 按 `#[track_caller]` 的**直接调用点文件**归组，若写在 `world.rs`，
//! 同文件里既有的 `MabMabe`（`world.rs:1840`）与攻击路径取数会被这条宽 pattern 误吞。
//!
//! | 组（`consumer_groups.tsv`） | C# oracle | Rust |
//! | --- | --- | --- |
//! | `player_run` | `PlayObject.Run@PlayObject.Message.cs:283`（`CheckDupObj`） | 本文件 |
//!
//! 卡：`openmir2-run-gap-checkdupobj`（R 级）。

use mir2_shared::rng::RandomSource;

/// `PlayObject.Message.cs:283` 的 `M2Share.RandomNumber.RandomByte(8)`。
///
/// 语义（逐句核过）：
/// - C# `RandomNumber.RandomByte(byte value)`（`src/OpenMir2/RandomNumber.cs:117-124`）：
///   `value < 255` ⇒ `(byte)random.Next(value)` ⇒ **恰好一次取数**，值域 `0..=7`。
/// - Rust `RandomSource::random_byte(8)` ⇒ `random_below(8)` ⇒ `DotNetRandom::next_max(8)`
///   （`crates/shared/src/rng.rs`）——与 .NET `Random.Next(8)` 同源、同为**一次**采样。
///
/// 只在三处门全部通过后调用（oracle 的取数在 `CharPushed` 的实参位置，见
/// [`crate::World::check_dup_obj`]）⇒ 门的任何变化都会改变取数次数/位置。
pub fn dup_push_dir(rng: &mut impl RandomSource) -> u8 {
    rng.random_byte(8)
}
