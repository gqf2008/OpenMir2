# characters_item.Dura 非确定性裁决（T-269，2026-10-29）

卡：`openmir2-dura-nondeterminism-adjudication`；**tier=R，`window_required=true`**。
关联：`openmir2-rng-divergence-before-weapon-wear`（T-266 / 攻击窗口归属分析）。

## 结论

`characters_item.Dura` 的跨实现差异**不是 Rust 实现差**，而是 oracle 自身的 run-to-run
非确定性。同 seed、同二进制、同场景的两次 C# 运行，末次 `Random(5)` 抽值不同 ⇒ 落库 Dura
不同；Rust 单线程两次都稳定在 3994：

| 运行 | C# 末次 Random(5) | C# 磨损 | C# DB Dura | Rust Random(5) | Rust DB Dura |
|---|---:|---:|---:|---:|---:|
| run9 | 3 | 5 | 3995 | 4 | 3994 |
| run13 | 0 | 2 | 3998 | 4 | 3994 |

机制：`nWeaponDamage = (ushort)(Random(5) + 2 − WeaponStrong)`
（`src/M2Server/Player/PlayObject.Attack.cs:225`；WeaponStrong=0 ⇒ 磨损 2..6），
`DoDamageWeapon` 扣 Dura 并按尾桶门发 642（`:1436-1468`）。该次抽数来自**共享全局 RNG**
（`src/OpenMir2/RandomNumber.cs:83-100`，无锁），而 C# 怪物 AI 由多个后台线程驱动
（`src/GameSrv/Word/WorldServer.MonGen.cs:145-160`）⇒ 全局流是并发交错，末次抽值本身
不是跨实现契约。

**处置**：登记为**受控非契约**（T-269），不做全局 `ignore_columns`、不做整列掩码；
只放行「武器槽那一行、`Dura` 这一格」，而且必须逐次通过本侧证据见证。

## 规则（`tools/conformance/rules.json` → `db.adjudications[T-269]`）

```json
{
  "id": "T-269",
  "table": "mir2_db.characters_item",
  "column": "Dura",
  "row_match": {"Position": "1"},
  "witness": {
    "frame_ident": 642, "frame_param": "1", "frame_count": 1,
    "rng_canonical": "attack_success", "rng_count": 1,
    "wear_offset": 2, "wear_range": [2, 6],
    "require_db_equals_frame_recog": true,
    "require_duramax_minus_recog_equals_wear": true
  },
  "tier": "R", "card": "openmir2-dura-nondeterminism-adjudication"
}
```

`compare_runs.apply_db_adjudications` 对每条 `value` 差异做**行级**判定：

1. 按主键定位这条差异行，两侧都必须满足 `row_match`（Position=1 的武器槽）；否则照旧判红
   （非武器槽行即便数值恰好等于见证，也不放行）。
2. 本侧 `frames.ndjson` 里 `ident=642` 必须**恰好 1 条**，`param=1`；
   DB 值必须等于该 642 的 `recog`。
3. 本侧 `rng-stream.tsv` 必须存在且可解析，`attack_success` 恰好 1 抽；
   末次抽值 `r` 满足 `wear = r + 2 ∈ [2,6]`，且 `DuraMax(=642 tag) − recog == wear`。
4. 见证成立 ⇒ 该单元格差异转成 `whitelisted`（`kind=db_adjudicated_nondeterminism`，
   带 `adjudication=T-269`、卡号、原因、两次 C# 证据、两侧 db/642/random5/wear）；
   矛盾（DB≠642、642≠1 条、磨损越界、公式不符）⇒ 保留差异并追加
   `adjudication_violation`（红）；证据缺失（无 rng 流水、定位不到行）⇒ UNDECIDABLE（rc 2）。

站点 canonical 复用 `mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py`，不抄第二份表。

## 离线重算（A4）

```sh
python -I -B tools/conformance/compare_runs.py \
  --oracle E:/tmp/window/run9/equip_struck_save/csharp \
  --rust   E:/tmp/window/run9/equip_struck_save/rust  --out E:/tmp/dev-2/dura-run9-report.json
```

- **run9**：`rc=0 / PASS`，`db.diffs=[]`，帧面全等；唯一放行项是
  `db_adjudicated_nondeterminism`（oracle db=3995/642=3995/random5=3/wear=5；
  rust db=3994/642=3994/random5=4/wear=6）。
  report sha256 `4723e98f6ccf10f5eb003515760370dc979badbf2d6aa704aa897a57fd7676f1`。
- **run13**：Dura 同样被正确登记（C# 3998/random5=0/wear=2；Rust 3994/random5=4/wear=6），
  `db.diffs=[]`；但仍有 **1 条帧面差异：Rust 多一条 656**（`block_align`，36 vs 37 帧）。
  这是另一张卡负责的 656 偶发缺失，按本卡 `forbidden` **不得并入、也不得被本规则吞掉** ⇒
  保持 `rc=1 / FAIL`。report sha256
  `97ebd6ad55df6cace7eed5c9fb3c4733f33433a90cd4259fd3a321bf17720dd5`。

输入哈希：run9 C# `rng-stream.tsv` sha256 `4cf79289…`；run13 C# `rng-stream.tsv`
sha256 `f41c321c…`。

## 负例与变异牙

`compare_runs.py selftest` 新增 10 项（GREEN 377/377）：

| 用例 | 断言 |
|---|---|
| 正例 | 见证成立 ⇒ rc 0，`whitelisted` 登记 T-269 且带两次 C# 证据、`db.diffs` 空 |
| DB≠本侧 642 | rc 1 + `adjudication_violation` |
| 642 条数 ≠1 | rc 1（无法绑定唯一一次攻击） |
| Random(5)=9 | 磨损 11 越界 ⇒ rc 1 |
| DuraMax−recog ≠ Random(5)+2 | rc 1 |
| 非武器槽行 Dura 差异 | rc 1（行级作用域，key=107） |
| DuraMax 列差异 | rc 1（同表其它列不受裁决影响） |
| 缺 rng-stream.tsv | rc 2 + `db_adjudication_undecidable` |
| 删掉 `db.adjudications` | 原有 Dura 差异照旧 rc 1（删判据即红） |
| witness 未知键 | 加载期 rc 2（不许声明没实现的判据） |

变异复验（逐条应用 → selftest 命名红 → 还原后 sha256 一致、`git status` 空）：
关闭裁决应用 / 删 DB==642 / 删磨损值域 / 删 DuraMax 公式 / 删行级 row_match /
缺 rng 降成红 / 不并入 whitelisted —— **7/7 全部咬红**。

## 边界与残留

- `WeaponStrong=0` 是本场景夹具事实（T-266 的输入端已接线）；若某场景 WeaponStrong≠0，
  `DuraMax−recog == Random(5)+2` 会不成立 ⇒ 判红，不会误放行（需要时另开规则）。
- 规则 `approved_by` 仍是 `PENDING owner/coordinator review` ⇒ 报告
  `rules_pending_review=true`；R 级收口仍需当前 master 的 n13 复跑与同 seed C# 两次复跑证据。
- 656 偶发缺失不在本卡：run13 的 `block_align` 差异按原样保留，由 656 专卡处理。
- 临时产物：`E:/tmp/dev-2/dura-run{9,13}-report.json`（证据，保留）、
  `mutate-dura.py`、`msg-dura1.txt`（过程产物，可删）。
