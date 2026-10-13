# evidence-m3-pet-slave-registry-20261013

`openmir2-m3-pet-slave-lifecycle`（M3⑨-3）**收口片**的证据包：落地 reviewer-5 的两条非阻塞 Minor
（**M-1** whitelist 行未落 / **M-2** `slave_list_cleanup_all` 调用点跨对象相对顺序）。

- base = `origin/master` `9c776129c`；分支 `worktree-m3-pet-slave-lifecycle-registry`。
- 本片**只读 / 零端口**：只改 `mir2-rs/tests/parity/whitelist.md`（+5 行）+ 新增本门脚本与证据；
  **零 `.rs`**、零 `src/ sql/ MirClient`、零 `crates/ *.rs`、未起栈、未占端口、未 merge、未自签。

| 文件 | 内容 | 命令 |
| --- | --- | --- |
| `00-oracle-anchors.txt` | 5 行登记行的 oracle 锚原文（`PlayObject.Base.cs:345-352`、`MonsterObject.cs:71-84`、`BaseObject.Base.cs:869-883/205-218`、`Grobal2.cs:21-25`、`ServerConf.cs:519-522` + `rg GainSlaveExp`） | `sed -n` / `grep -rn`（见文件内 `[cmd]`） |
| `01-whitelist-edit-diff.txt` | 本片对 `whitelist.md` 的**唯一**改动（B-112..B-115 + M3⑨ 实现登记块 T-49） | `git --no-pager diff -U2 9c776129c -- mir2-rs/tests/parity/whitelist.md` |
| `02-gate-check-green.txt` | 收口门**改后 GREEN**：5 行登记齐 + 锚点齐 + B 段编号唯一，rc=0 | `python -I m3_pet_slave_registry_gate.py check` |
| `03-gate-selftest.txt` | 门自检（合成文本 + 注入式 `check_text` + 改坏必红）**5/5 GREEN**，rc=0 | `… selftest` |
| `04-gate-red-pre-patch.txt` | **改前读数**（base `9c776129c` 的 whitelist，无本片行）⇒ `MISSING` ×5，rc=1 | `… check --whitelist <base 副本>` |
| `05-gate-red-external-mutation.txt` | **外部改坏必红**（临时副本，仓库文件不动）：删 B-115 ⇒ `MISSING`；锚点改错 ⇒ `ANCHOR`；追加重复 B-114 ⇒ `DUPLICATE`；三条 rc=1 | `… check --whitelist E:/tmp/m3hp-whitelist-mut.txt` |
| `06-gate-fmt.txt` | 四门禁①（cwd=`mir2-rs`）rc=0 | `cargo fmt --all -- --check` |
| `07-gate-clippy.txt` | 四门禁② rc=0 | `cargo clippy --workspace --all-targets --offline -- -D warnings` |
| `08-gate-test.txt` | 四门禁③ **99 suites / 1159 passed / 0 failed / 22 ignored** rc=0 | `cargo test --workspace --no-fail-fast --offline` |
| `09-gate-msgcodegen.txt` | 四门禁④ OK 604 rc=0 | `cargo run -p msgcodegen --offline -- --check` |

## 登记内容（便于审查对照）

| 号 | 落点 | oracle 锚 | 本片判据 |
| --- | --- | --- | --- |
| B-112 | `DC` 高字节截断（`(ushort)MakeLong(...)`） | `PlayObject.Base.cs:349` | 行内锚 `PlayObject.Base.cs:349` + `process_slave_mutiny` |
| B-113 | `MasterDieMutinySpeed` 除零（+`ServerConf.cs:521` 错键） | `PlayObject.Base.cs:350` | 行内锚 `:350` + `ServerConf.cs:521` |
| B-114 | `GainSlaveExp` 死代码（全仓无调用点） | `MonsterObject.cs:71-84` | 行内锚 + `rg 'GainSlaveExp' src/` 仅定义处（`00-`） |
| B-115 | 色表 latent 越界（门 50 vs 15 项表） | `BaseObject.Base.cs:877-881` + `Grobal2.cs:23` | 行内锚 `:877-881` + `Grobal2.cs:23` + `SlaveColor` |
| T-49 | 清表调用点集中到 `World::tick` + `Master.SlaveRelax` 未建模 | `BaseObject.Base.cs:208-217` + `MonsterObject.cs:251-360` | 行内锚 + `slave_list_cleanup_all` |

## 编号说明（不假绿）

- **T-48** 按已闭合卡 `openmir2-m3-whitelist-registry-integrity`（worker-5，合入 master `3b82a2fa`）的建议，
  留给「T-42 撞号去重」（whitelist 现存两行 `| T-42 |`：⑤-2 商人 tick / ⑥-5 任务夹具；该卡已路由 owner）
  ⇒ 本片取 **T-49**。
- **B-112..B-115** 为 B 段当前空号：早前 reviewer-1 曾建议 `B-112` 用于 811 Job 面，作者（m2svr 线）
  判定为 1:1、**未落行** ⇒ 该号空出，本片按「B 段 = 复刻的 C# 已知 bug」体例取用。
- 本门**不审** T-42 撞号（跨车道历史问题、已路由 owner）；T 段全局唯一性归
  `m3_whitelist_registry_audit.py`（那张卡的审计，非本片）。本门只对本片声明的 5 行 fail-closed。

口径：

- `SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / git blob）**记（`verify_sha256sums.py write`）。
- 四门禁在 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`、`--offline`、cwd=`mir2-rs` 下跑。
- 真机对拍（`run_hero_pet_pair.ps1`）仍等独占窗口，**不在本片**（卡上 `not_verified` 不变）。
