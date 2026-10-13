# ⑧-8 O-1 收口：`compare_lstr` 对齐 oracle 逐码元语义（不再字节切片 panic）

- 卡：`openmir2-m3-castle-door-repair`（M3⑧-实现 8）
- 分支：`worktree-m3-castle-door-repair-o1`
- **base**：`origin/master` = `9863d57b0`（`Merge 7589161cd (worktree-m3-fixture-logexit-literal)`）
- 写集：`mir2-rs/crates/world/src/castle.rs` + 本证据目录。零 `bins/`、零 `crates/envir`、零 `src/ sql/ MirClient`。
- 来源：reviewer-5 `2d159fd5b9`（⑧-8 片 review）**O-1（Minor）**。

## 1. O-1 是什么（移植偏差，不是 oracle 行为）

oracle `HUtil32.CompareLStr(string src, string targ)`（`src/OpenMir2/HUtil32.cs:550-576`）：

```csharp
int compn = targ.Length;          // UTF-16 码元数
if (compn <= 0) return false;
if (src.Length < compn) return false;
for (int i = 0; i < compn; i++)
    if (char.ToUpper(src[i]) == char.ToUpper(targ[i])) continue; else return false;
return true;
```

它**在 UTF-16 码元级**比较，越界只发生在码元层、**从不切字节** ⇒ 对任意输入都不抛。

Rust 旧实现（⑧-8 片，`origin/master:mir2-rs/crates/world/src/castle.rs`）：

```rust
pub fn compare_lstr(src: &str, targ: &str) -> bool {
    if targ.is_empty() || src.len() < targ.len() { return false; }
    src[..targ.len()].eq_ignore_ascii_case(targ)
}
```

`src[..targ.len()]` 是**字节切片**：当 `targ.len()` 恰好落在某个多字节 `char` 中间时按 Rust
语义 **panic**，而 oracle 不会 ⇒ 这是 Rust 移植缺陷，必须修到与 oracle 同形（`§1 行为等价`）。

## 2. 改法（最小、与 oracle 同形）

`compare_lstr` 改为逐 `char` 比较：

- `targ` 空 ⇒ `false`（oracle `compn <= 0`）；
- `src` 的字符数 < `targ` 的字符数 ⇒ `false`（oracle `src.Length < compn`）；
- 否则逐位 `sc.eq_ignore_ascii_case(&tc)`，全等 ⇒ `true`。

不变量：对任意 UTF-8 输入**都不 panic**；对 ASCII `targ`（本函数唯一调用面 = `HIRE_GUARD_NOW`
`@hireguardnow` / `HIRE_ARCHER_NOW` `@hirearchernow`，均 ASCII）与 oracle 逐码元结果一致
（首位/中位不等 ⇒ `false`）。ASCII 大小写折叠与既有口径一致（未改语义）。

> BMP 下 Rust `char` 与 UTF-16 码元一一对应；非 BMP `targ` 不在任何调用面上（当前/接线后均只喂 ASCII 常量）。

## 3. 判据与读数

- **真身向量**：新增 `castle::tests::compare_lstr_matches_oracle_prefix_semantics_without_panic`
  （`01b`）：含空 `targ` / src 短 / 全等 / 大小写折叠 / 前缀匹配 / 位同位不等，以及 5 条多字节边界
  向量（`@hireguardnoé` 是「`targ.len()`=13 落在 `é`(bytes 12..14) 中间」的**触发样本**）。
- **castle 模块**：`cargo test -p mir2-world --lib castle::` ⇒ **39 passed / 0 failed**（`01`）。
- **改坏必红**（`02`）：把实现改回 `src[..targ.len()].eq_ignore_ascii_case(targ)` ⇒
  该用例 **FAILED rc=101**，panic 点 `castle.rs:452:8`：`end byte index 13 is not a char
  boundary; it is inside 'é' (bytes 12..14 of string)`；`git checkout --` 还原后源 sha256 一致、
  重跑 GREEN。
- **四门禁**（在 base `9863d57b0` 的交付树上，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`）：
  `03` fmt rc=0 / `04` clippy `-D warnings` rc=0 / `05` **101 suites 1190 passed 0 failed 22 ignored** /
  `06` msgcodegen **604** rc=0。
- **边界**（`07`）：仅 `crates/world/src/castle.rs` + 本目录；冻结面 `src/ sql/ MirClient
  crates/protocol crates/storage bins/` **空**。
- **证据包自证**：`python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <本目录>`
  ⇒ `GREEN [repo 口径] OK 10/10` rc=0；`... eolcheck <本目录>` ⇒ `EOLCHECK GREEN 11/11` rc=0
  （LF 归一 / git blob 口径）。

## 4. 未做（如实登记）

- ⑧-8 卡级剩余 DoD 不变：第②片 `bins/game-svr` 门/墙 tick 接线（等 owner 对 bins 串行序裁决）+
  真机两侧 `run_guild_castle_pair.ps1` diff=0（归 ⑧-10 夹具 + 独占窗口）。
- 本片**只**收口 O-1，未扩大 ⑧-8 其它面；未新增/修改 oracle、表、脚本、协议、客户端。

## 5. 残留物清单

- 本目录（证据件，10 件）为本片专属，保留。
- 临时物：无（隔离编译产物在 `E:/tmp/openmir2-m3cs-target/`，为车道共享构建目录，非本片专属）。
