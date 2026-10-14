# evidence-m3-guild-war-quirk-reg-20261014

`openmir2-m3-guild-war`（M3⑧-5）**收口片**的证据包：落地 reviewer-6 对第①片 approve
（`0fce7a8c` @ `reviewed_head efa75a9dee`）登记的**非阻塞 Minor O-1** ——
「先扣钱后判掌门」的照搬怪行为当时只在 `crates/world/src/guild.rs` 模块头 + 证据 README 说明，
`whitelist.md` 没有对应行 ⇒「登记在案」不可机判、将来被「修好」不会被发现。

- base = `origin/master` `057d7dc9e597ec0b4c2c6d3f7f9feb4993b206ce`；分支 `worktree-m3-guild-war-quirk-reg`。
- 本片**只读 / 零端口**：只改 `mir2-rs/tests/parity/whitelist.md`（+T-54 行）+
  `mir2-rs/tests/parity/m3/fixture_chain_gate.py`（新门接入共享管线）+ 新增
  `mir2-rs/tests/parity/m3/m3_guild_war_quirk_registry_gate.py` + 本证据包；
  **零 `.rs`**、零 `src/ sql/ MirClient`、零 `crates/**`、未起栈、未占端口、未 merge、未自签。

| 文件 | 内容 | 命令 |
| --- | --- | --- |
| `00-oracle-anchors.txt` | 登记行的 oracle 锚原文（`GuildOfficial.cs:180-199` 先 `DecGold`、`PlayObject.Base.cs:3349-3385` 后判 `IsGuildMaster`、`PlayObject.cs:4400-4405` 定义）+ Rust 锁定用例名 | `sed -n` / `rg -n`（见文件内 `[cmd]`） |
| `01-code-diff.txt` | 本片代码差量（`whitelist.md` + `fixture_chain_gate.py`）+ 新门全文 | `git --no-pager diff <base> HEAD -- …` / `git show HEAD:…` |
| `02-gate-check-green.txt` | 收口门**改后 GREEN**：T-54 行在 + 6 锚点齐 + 编号唯一，rc=0 | `python -I m3_guild_war_quirk_registry_gate.py check` |
| `03-gate-selftest.txt` | 门自检（合成文本 + 注入式 `check_text` + 改坏必红）**6/6 GREEN**，rc=0 | `… selftest` |
| `04-gate-generic-audit.txt` | 通用登记号审计（`audit` 非 strict 报告 + `selftest` 9/9） | `python -I m3_whitelist_registry_audit.py audit` / `selftest` |
| `05-gate-red-pre-patch.txt` | **改前读数**（base 的 whitelist，无 T-54）⇒ `MISSING`，rc=1 | `… check --whitelist <base 副本>` |
| `06-gate-red-external-mutation.txt` | **外部改坏必红**（临时副本，仓库文件不动）：删行 ⇒ `MISSING`；锚改错 ⇒ `ANCHOR`；追加重复 ⇒ `DUPLICATE`；删 Rust 锁定用例名 ⇒ `ANCHOR`；四条 rc=1 | `… check --whitelist E:/tmp/…` |
| `07-chain-gate-selftest.txt` | 链深检门自检 **38/38 GREEN**（含本片新增防回退牙：行会战怪行为登记门必须常驻共享管线） | `python -I fixture_chain_gate.py selftest` |
| `08-chain-gate-check.txt` | 链深检门 check：本片新门 `selftest 6/6` + `check GREEN`；B（Claim B）RED 见 `09-` | `python -I fixture_chain_gate.py check` |
| `09-chain-red-preexisting-base-proof.txt` | 在 base 的临时 detached worktree 上跑 B runner ⇒ 同 `anchors-freshness-selftest` RED ⇒ **改前既有、与本片无关** | `powershell -File rng-trace-scene/run_rng_trace_pair.ps1 -SelfCheckNoStack` |
| `10-gate-fmt.txt` | 四门禁① rc=0 | `cargo fmt --all -- --check` |
| `11-gate-clippy.txt` | 四门禁② rc=0 | `cargo clippy --workspace --all-targets --offline -- -D warnings` |
| `12-gate-test.txt` / `12b-gate-test-summary.txt` | 四门禁③ **101 suites / 1213 passed / 0 failed / 22 ignored** rc=0 | `cargo test --workspace --no-fail-fast --offline` |
| `13-gate-msgcodegen.txt` | 四门禁④ OK 604 rc=0 | `cargo run -p msgcodegen --offline -- --check` |
| `14-boundary.txt` | 差量清单 + 冻结面 / `.rs` 空 | `git diff --name-status origin/master`（`git add -N` 后含未跟踪件） |

## 登记内容（便于审查对照）

| 号 | 落点 | oracle 锚 | Rust 锁定用例 |
| --- | --- | --- | --- |
| T-54 | 「先扣钱后判掌门」照搬怪行为（非掌门 / 非 0 区 / 目标缺失都白扣 `GuildWarPrice`） | `GuildOfficial.cs:186-187`（`DecGold`）+ `PlayObject.Base.cs:3351`（`IsGuildMaster`）+ `PlayObject.cs:4402`（定义） | `request_guild_war_non_master_still_pays_gold` / `request_guild_war_no_guild_still_pays_gold` / `request_guild_war_non_zero_server_still_pays_gold` |

## 编号说明（不假绿）

- **T-48** 按已闭合卡 `openmir2-m3-whitelist-registry-integrity`（worker-5，合入 master `3b82a2fa`）的建议，
  留给「T-42 撞号去重」（whitelist 现存两行 `| T-42 |`：⑤-2 商人 tick / ⑥-5 任务夹具；已路由 owner）。
- **T-49**（⑨-3）/ **T-50** / **T-51**（④ 夹具）已用；**T-52** / **T-53** 由卡
  `openmir2-m3-party-recall-ext`（worker-21，已合入 master `2d2426f5b`，closed）占用 ⇒ 本行取 **T-54**。
- 本门**不审** T-42 撞号（跨车道历史问题、已路由 owner）；T 段全局唯一性归
  `m3_whitelist_registry_audit.py`（那张卡的审计，非本片）。本门只对本片声明的登记合同 fail-closed。

口径：

- `SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / git blob）**记（`verify_sha256sums.py write`）。
- 四门禁在 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`、`--offline`、cwd=`mir2-rs` 下跑。
- 真机对拍（`run_guild_castle_pair.ps1` 同序列 diff=0）仍等 ⑧ 实现卡 + 时钟注入 + 独占窗口，
  **不在本片**（卡上 `not_verified` 不变）。
- 本片不改行为、不改 `src/**`/`crates/**`；只补登记行 + 机判门。
