# evidence-m3-pet-slave-m3-rc-eol-20261013

**卡**：`openmir2-m3-pet-slave-summon`（M3⑨-2）· **finding**：reviewer-6 在 approve
`35ad2fb8cb @4f5ad7134` 登记的 **M-3（Minor，非阻塞）**。

## 发现（reviewer-6 M-3，原文口径）

> F-1 证据新增 `redchecks/RC5-*.rc` 的 `.rc` 扩展名未在 `mir2-rs/.gitattributes` 声明 ⇒ 干净检出上
> eolcheck **RED 2/19**；修法 = 改名 `.rc.txt` 或补 `*.rc text eol=lf`。

## 修法（选最小侵入：补属性，不改任何被跟踪 blob）

`mir2-rs/.gitattributes` 证据扩展名段追加一行：

```
*.rc     text eol=lf
```

（附理由注释；`*.rc` 在本仓**仅**这 2 件证据文件 —— `git ls-tree -r HEAD | grep '\.rc$'` 全树只有
`.../evidence-m3-pet-slave-f1-rework-20261013/redchecks/RC5-*.rc`，均为一行的 red-check 退出码标记
`mutated/restored rc=<n>`。全树无其他 `.rc`，故不会误伤 Windows 资源脚本类文件。）

未选“改名 `.rc.txt`”：属性口径是可复用的主动牙（先例卡 `openmir2-gitattributes-eol-coverage` 已用同法覆盖
`*.py/*.tsv/*.ps1/*.patch/*.ndjson/*.js`，只是漏了 `.rc`），改名则把同类问题留给下一个扩展名。

## 判据（机器可判，改坏必红）

| 判据 | before（8aebbcb85） | after（bc2118446） |
| --- | --- | --- |
| 新鲜检出 `eolcheck <本 finding 的证据目录>` | **RED 2/19**（rc=1） | **GREEN 19/19**（rc=0） |
| 新鲜检出 `eolcheck mir2-rs`（全树） | **RED 2/3592**（rc=1） | **GREEN 3592/3592**（rc=0） |
| 改坏（删 `*.rc` 行 + 强制重落盘 `.rc`） | — | **RED 2/19**（rc=1，见 `03-break-must-be-red.txt`） |

“新鲜检出”= `git worktree add --detach <dir> <commit>`（attribute 只影响**新检出**；既有工作树里已落盘的
CRLF 不会被回溯归一，故必须用新 worktree 复现）。before/after 两侧均在 `core.autocrlf=true` 下
（本机 Git for Windows 默认）。

## 证据文件

| 文件 | 内容 |
| --- | --- |
| `01-before-red.txt` | before：全树 RED 2/3592 + finding 目录 RED 2/19 |
| `02-after-green.txt` | after：全树 GREEN 3592/3592 + finding 目录 GREEN 19/19 |
| `03-break-must-be-red.txt` | 删 `*.rc` 行后强制重落盘 ⇒ RED 2/19（rc=1） |
| `04-boundary.txt` | 边界（对 `src sql MirClient` 空；代码 delta 仅 `.gitattributes`） |
| `gates/` | 四门禁（本片检出，隔离 target）：fmt / clippy / test / msgcodegen |

oracle 锚：本片为**仓库卫生**修复，不涉 oracle 行为 ⇒ 无 C# 行为锚；锚 = reviewer-6 M-3 finding +
先例卡 `openmir2-gitattributes-eol-coverage`（`.gitattributes` 证据扩展名段）与
`openmir2-tools-eol-attributes`（`tools/` 侧同族牙）。

## 边界 / 非目标

- **非目标**：不改任何被跟踪 blob 内容（`.rc` 的 blob 本就是 LF）；不实现 save；不动 oracle（`src/`）、
  `sql/`、`MirClient`；不碰生产代码。
- `git diff --name-only 8aebbcb85 bc2118446 -- src sql MirClient` = **空**。
- 代码 delta（base→code head）= 仅 `mir2-rs/.gitattributes`（+8 行，属性/注释）。
- 本证据 commit 为 `bc2118446` 的子提交，只含本目录。

## 门禁（`CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`，检出外；`mir2-rs/` 下运行）

- `cargo fmt --all -- --check` rc=0
- `cargo clippy --workspace --all-targets -- -D warnings` rc=0
- `cargo test --workspace --no-fail-fast` = **98 suites · 1128 passed · 0 failed · 21 ignored** rc=0
- `cargo run -q -p msgcodegen -- --check` = **604 条常量** rc=0

## 残留物

无。本线未起游戏栈、未占端口；scratch worktree `E:/tmp/m3hp-eol-{repro,after}` 与本目录外的
临时输出用完即删（内容已入库本目录）。
