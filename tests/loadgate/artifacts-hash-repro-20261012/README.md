# F9-1 修复证据：`artifacts.sha256.json` 在全新检出上 0 mismatch（2026-10-12）

- 卡：`openmir2-c6-artifacts-hash-repro`（owner `openmir2-worker-18`，worktree `c6h`，branch `worktree-c6-artifacts-hash`）
- 改法：**选 (a)** —— `tests/loadgate/.gitattributes` 的 `* -text` **冻结字节**；三个 `artifacts.sha256.json`
  按**提交字节**重算（只改 `sha256` 值，被引用的产物一个字节没动）；新增复核器
  `tools/botload/verify_artifacts.py`；README 写清口径与复核命令。
- base = master `15cec558`。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-before-recompute-lf.txt` | 修复前：在 LF 工作树上重算三个旧清单 ⇒ **36/105 mismatch**（E 18/45、A/C 各 9/30） |
| `02-regenerate-manifests.txt` | 按提交字节重算三个清单（逐条列出旧/新 sha256；共 36 条） |
| `03-after-recompute.txt` | 修复后：**0 mismatch**；`git status` 只剩意图内改动；`tests/loadgate` 222 件 `w/lf` |
| `04-redcheck-one-byte.txt` | **牙齿**：改掉 `165805/run-1/tier-200/report.json` 一个字节 ⇒ `[MISMATCH]`；`git checkout` 还原 ⇒ 0 mismatch、worktree 干净 |
| `05-load_report-selftest.txt` | `python tools/botload/load_report.py selftest` ⇒ rc=0（GREEN） |
| `06-four-gates.txt` | 四门禁（隔离 `CARGO_TARGET_DIR=/e/tmp/openmir2-c6h-target`，`--offline`）：fmt / clippy `-D warnings` / `cargo test --workspace` / `msgcodegen --check (604)` 全 rc=0 |
| `07-fresh-checkout.txt` | **②可复现判据**：`git clone` 到临时目录的**全新检出**上按 README 复核 ⇒ 0 mismatch |

## 哈希口径（单一真源）

    sha256 = SHA256(文件的原始字节)，不做任何 EOL / 编码归一化。

`tests/loadgate/.gitattributes` 的 `* -text` 让 git **原样存、原样检出**（不受 `core.autocrlf` 影响），
所以清单生成当时哈希的字节 = 提交的字节 = 任何全新检出得到的字节。

复核命令（任意检出）：

```sh
python tools/botload/verify_artifacts.py tests/loadgate/load-*/artifacts.sha256.json
# 期望：OK 3/3 清单, 105/105 文件，0 mismatch，rc=0
```

> `load_gate_summary.json` 内嵌的 `artifacts` 数组是**修复前**口径的历史快照，勿用于检出核对；
> 权威清单 = 同目录 `artifacts.sha256.json`。详见 `../README.md》产物清单哈希口径与复核`。
