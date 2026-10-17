//! `BaseObject.SpaceMoveGetRandXY` 的独立模块（`mir2-rs` 侧）。
//!
//! **为什么单独成文件（2026-10-17）**：这条取数被**两个** oracle 函数共用——`BaseObject.RecallSlave`
//! （`BaseObject.cs:1850`，魔号 30 回落）与 `BaseObject.MapRandomMove`（`:1214`，商人随机移动）——而
//! RNG 结构一致性门（`rng-trace-probe/consumer_groups.tsv` 的 `groups --require-closed`）按**站点字符串**
//! 归组。若它留在 `slave.rs`，则只能整文件归组，会把**同文件的另一个 oracle 函数**
//! （`PlayObject.ProcessSlaveMutiny`，`PlayObject.Base.cs:344`）的抽数一并吞进同一组 ⇒ 真实机 trace
//! 在该场景会与 C# 侧分组不一致（假红）。单独成文件后，两侧都按**函数**对齐：
//! `space_move` 组 = 本文件；`slave_mutiny` 组 = `slave.rs`。
//!
//! 公开 API 未变：`crate::slave::space_move_get_rand_xy` 仍可用（`slave.rs` 里 `pub use` 再导出）。

use mir2_shared::rng::RandomNumber;

/// `BaseObject.SpaceMoveGetRandXY`（`BaseObject.cs:1040-1096`）：从 `(x, y)` 起找可走格；
/// 不可走时**按 oracle 的分段步进/随机跳点**推进，最多 201 次尝试（`n14 >= 201` 即放弃）。
///
/// `n18 = Width < 80 ? 3 : 10`；`n1C = Height < 150 ? (Height < 50 ? 2 : 15) : 50`。
/// `can_walk(x, y)` = oracle `Envir.CanWalk(x, y, true)`（`cellSuccess && Valid`）。
///
/// **取数次序是 Claim B 账本输入**：只有进入 `else` 分支（`x` 已到 `Width-n1C-1` 之后）才抽
/// `Random(Width)` / `Random(Height)`，本函数逐字保留分支位置。
pub fn space_move_get_rand_xy<F>(
    rng: &mut RandomNumber,
    width: i32,
    height: i32,
    mut x: i32,
    mut y: i32,
    mut can_walk: F,
) -> Option<(i32, i32)>
where
    F: FnMut(i32, i32) -> bool,
{
    let n18 = if width < 80 { 3 } else { 10 };
    let n1c = if height < 150 {
        if height < 50 {
            2
        } else {
            15
        }
    } else {
        50
    };
    let mut attempts = 0;
    loop {
        if can_walk(x, y) {
            return Some((x, y));
        }
        if x < (width - n1c - 1) {
            x += n18;
        } else {
            x = rng.random_below(width);
            if y < (height - n1c - 1) {
                y += n18;
            } else {
                y = rng.random_below(height);
            }
        }
        attempts += 1;
        if attempts >= 201 {
            break;
        }
    }
    None
}
