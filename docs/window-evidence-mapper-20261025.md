# 窗口证据映射器（card evidence mapper）— 2026-10-25

- 卡：`openmir2-window-evidence-mapper`（tier **L**，owner `openmir2-dev-2`，branch `card/window-evidence-mapper`）。
- 定位：把条件② 的「卡面判据」从散文变成**机器可判**：一张判据表（`tools/window/card-criteria.json`）+ 一个只读运行器
  （`tools/window/card_evidence.py`），对**已采到的帧证据根**逐条给出 `PASS / FAIL / UNDECIDABLE`。
- 红线：**UNDECIDABLE 绝不是 PASS**；缺数据/缺字段/空值一律不通过；不按实得帧回填期望值；
  运行器**只读**（不起服务端、不占端口、不写证据根）。本卡不判 `closed`，只交付工具与判据表。

## 1. 判据表 schema（`card-criteria/1`）

```json
{
  "format": "card-criteria/1",
  "criteria": [
    {
      "card": "openmir2-…",            // 与看板卡 id 一致（台账 §1 的「卡 id」列）
      "w_item": "W-…",                 // 台账 §1 的 W 编号，**逐字照抄**，不自造
      "scenario": "…",                 // 证据根应属的场景名（run.json 的 scenario.name）
      "source": "docs/…§…",            // 期望值的出处（oracle 代码 / 台账 / 已合并 R 片）
      "assert": {
        "ident": "SM_GOLDCHANGED",     // 消息名（由 mir2-rs/crates/protocol/src/messages.rs 解析）或数字
        "recog": 9912,                 // 可选：帧的 recog 必须等于此值
        "min_count": 2,                // 可选：该 ident 至少出现几次
        "order": ["SM_A", "SM_B"],     // 可选：这些 ident 的**首个** seq 必须严格递增（同一证据文件内）
        "fields": [                    // 可空；空 = 只断言 ident 存在/次数/序
          {"path": "param", "expect": 0, "cmp": "eq"},
          {"path": "body_text", "expect": "…", "cmp": "contains", "mask": "datetime"}
        ]
      }
    },
    {
      "card": "openmir2-…",
      "w_item": "W-…",
      "scenario": "…",
      "source": "docs/…§…",
      "machine": false,                // 本轮不可机判：必须带 reason（否则表不合法，rc=2）
      "reason": "缺什么 / 为什么"       // 运行器输出 UNDECIDABLE + 该原因，**不许静默跳过**
    }
  ]
}
```

- `cmp ∈ {eq, ne, ge, le, contains, regex}`；`mask ∈ {datetime, digits, none}`。
  `datetime` 把 `YYYY-MM-DD[ HH:MM:SS]` 抹成 `<DATETIME>` 再比（`$DATETIME` 类字段必须用它）；`digits` 抹掉所有数字串。
- 字段比较取**该 ident 最后一帧**；`recog` 过滤发生在字段比较之前。
- 表不合法（缺字段、未知 `cmp`/`mask`、`machine:false` 无 `reason`）⇒ 运行器 **rc=2**（fail-closed，不会退化成「全 UNDECIDABLE」）。

## 2. 运行器用法与退出码

```sh
python -I -B tools/window/card_evidence.py --run-dir <证据根> [--scenario <名>] \
    [--criteria tools/window/card-criteria.json] [--json out.json]
python -I -B tools/window/card_evidence.py --selftest
```

| 退出码 | 含义 |
|---:|---|
| 0 | 无 FAIL（其余为 PASS / UNDECIDABLE） |
| 1 | 至少一条 FAIL |
| 2 | 用法错误 / 证据根读不到（无 `frames.ndjson`）/ 判据表不合法 / 未知消息名 |

**证据根**：目录自身或任意子目录下的 `frames.ndjson`（递归收集，最多一次读全）；每帧的键即探针/抓帧格式
`seq, t_ms, phase, conn, ident, recog, param, tag, series, body_len, body_hex, body_sha256, frame_class, tail, raw_len, body_text`。

**场景归属（防「拿 A 场景的证据判 B 场景的卡」）**：
- 优先读证据根的 `run.json` 的 `scenario.name`（探针产物自带，**不从目录名猜**）；没有则需 `--scenario`；
- 两者皆无 ⇒ 打印警告，结果标 `provenance: unverified`，**不得当窗口证据**；
- 判据的 `scenario` 与证据根不一致 ⇒ 该条 **UNDECIDABLE「场景不符」**（不是 PASS，也不是 FAIL）。

## 3. 三态语义（钉死）

| verdict | 触发条件 |
|---|---|
| PASS | 场景相符；`order` 成立；该 ident 达到 `min_count`；`recog`（若声明）命中；**所有**字段存在且比较成立 |
| FAIL | 场景相符、数据齐，但 `order` 不成立 / `recog` 不命中 / 任一字段比较不成立 |
| UNDECIDABLE | `machine:false`（人工判据）；场景不符；ident 不在证据根；次数不足 `min_count`；字段缺失、`null`、空串；无法比较（如对非数字做 `ge`）；`order` 里任一 ident 缺 |

**注意 `0` 不是空值**：字段值 `0` 是有效数据，照常比较（selftest 有对照牙）。
唯一无法用本运行器表达的是「null 与 0 的语义区分」——帧里缺该键才记缺失。

## 4. 与 `compare_runs.py --rules` 的分工

| 工具 | 判什么 | 输入 | 输出 |
|---|---|---|---|
| `tools/window/compare_runs.py --rules` | **两侧**（C#/Rust）同场景的 ident 级序列 diff | 两个证据根 + rules.json | diff=0 / 差异清单（跨实现的等价性） |
| `tools/window/card_evidence.py` | **单侧**证据上「卡面判据」是否成立 | 一个证据根 + 判据表 | 逐条 PASS/FAIL/UNDECIDABLE（卡面可判性） |

两者不互相替代：`diff=0` 说明两侧一致，**不**说明卡面 W 项的数字成立；判据 PASS 说明单侧满足了钉住的期望，
**不**说明两侧一致。窗口收口需要两者都绿 + 台账 §3 的文件/DB 面（那部分本工具不覆盖）。

## 5. 台账 §1 九行的覆盖（W 编号逐字，逐行有处置）

「机判」= 已进 `card-criteria.json` 且判据来自台账/oracle 的钉住数字；「人工」= `machine:false` + `reason`，
运行器输出 UNDECIDABLE 并带原因。**没有静默跳过的行**。

| 台账 # | 卡 | W 项 | 本轮处置 | 缺什么 / 何时能机判 |
|---:|---|---|---|---|
| 1 | `openmir2-castle-merchant-tax-wiring` | W-TAX-1 | **机判**：`SM_BUYITEM_SUCCESS.recog=9912` 且 `param=0`（scenario `castle_merchant_tax_potion`） | 已有证据根即可判；买价 88 / 税 4 在文件面，见 W-TAX-2 |
| 1 | 同上 | W-TAX-2 / W-TAX-3 / W-TAX-4 | 人工（3 条各带原因） | 需配置副本（`Castle/List.txt` 两城 0,1）+ 620s 后 `SabukW.txt` 字段 + 可注入时钟（C# 无 `MIR2_CLOCK_*`）；W-TAX-3 前半冻结内容无「不在攻城区」商人 ⇒ 只能构造级 |
| 2 | `openmir2-loader-fields` | W-LF-1 / W-LF-2 | 人工 | 需 `newchr` 步（probe 无）+ 出生点 11 键钉值；W-LF-2 消费点未接线 |
| 3 | `openmir2-script-playdice-openybdeal` | W-PD-1 | **机判**：`SM_GAMEGOLDNAME` 出现（scenario `gamepoint_exchange`）+ 人工 1 条（DB 后态 / 5008 字节未钉值） | 5008 的 `Param/Tag` 台账只写「逐字节」未给值 ⇒ 先钉值 |
| 3 | 同上 | W-PD-2 | 人工 | 部署无 `@back`（非目标）；`@back2` 属部署内容差异 |
| 4 | `openmir2-clearmapmon-impl` | W-CM-2 | **机判**：`SM_DISAPPEAR` 出现（scenario `clearmapmon_q001`）+ 人工 1 条（实体/AOI 差集、坐标） | 坐标/实体面需按 oracle 口径解码帧尾（本轮无解码器） |
| 4 | 同上 | W-CM-1 | 人工 | 场景 blocked（`t118` 的 `movr M3` 不可注入）⇒ 无证据可采 |
| 5 | `openmir2-m3-castle-siege-state` | W-T127-1 | 人工 | `castle_zone` fixture 未建；相对序需与文本/收件人绑定且未钉字面值 |
| 6 | `openmir2-inbound-dispatch-red-batch` | W-DG-1 | **机判**：`SM_GOLDCHANGED.recog=100` 且 `param=0`（scenario `dropgold_zero_draw`）+ 人工 1 条（金币净差/地面物件/RNG 账本） | 金币差与地面物件在 DB/地图侧；RNG 流位需 `MIR2_RNG_LOG` |
| 6 | 同上 | W-DG-2 | 人工 | 场景 blocked（唯一缺口 = 场景内击杀步） |
| 7 | `openmir2-castle-official-design` | W-CO-1 | 人工 | `@main` 说词字面未钉住（只给 mask 规则）+ castle fixture 未建 |
| 8 | `openmir2-castle-official-actions` | W-CO-6 | 人工 | 需 owner 批准测试专用城主注入 + 可注入时钟（60s 严格 `>`） |
| 9 | `openmir2-script-handler-msgqueue` | W-MQ-1 | **机判**：帧序 `643 → 767 → 44 → 645 → 5008`（scenario `getbackupgnow`） | 有该场景证据根即可判；本轮无该根 ⇒ UNDECIDABLE「ident 未齐」 |
| 9 | 同上 | W-MQ-2 | 人工 | 出队序 `[0,2,1]` 需「同标签 3 条同 ident 帧 + 负载文本」分组语义，runner 暂无 |

> 台账里 #1 的 `castle_merchant_tax`（#22）与 `butch_two_draws`（#21）场景本身 blocked，不影响判据表的存在性——
> 判据一旦有证据根就能判；无证据根只会 UNDECIDABLE，不会 PASS。

## 6. A 类 49 卡的逐卡状态（`rcard-evidence-plan` §2）

选取规则（对应卡面 ①）：A 类 49 行里，「依据要点」含**钉住数字/帧序**的 6 张进机判；`npc-ctor-constants`
（series 低/高 word）标「机判受限」；其余 42 张逐卡给出「本轮为何不可机判」的归类，**不删行、不合并**。

| # | A 类卡 | §2 场景 / 夹具 | 本轮状态 |
|---:|---|---|---|
| 1 | `openmir2-a09-upgrade-item` | 新：R2 upgrade_retrieve | 场景未建（新场景/新夹具） |
| 2 | `openmir2-account-protection-13` | run_account_protection_roundtrip.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 3 | `openmir2-attack-weapon-wear-642` | `equip_struck_save`（§7.2 明确移入 §7.4） | 需坐标/实体解码或尚无钉值 |
| 4 | `openmir2-buff-system` | 新：R5 spell_cast + DoT trace | 场景未建（新场景/新夹具） |
| 5 | `openmir2-card-window-tool` | card_window.py 真 alt 剖面一次 | 需坐标/实体解码或尚无钉值 |
| 6 | `openmir2-click-path-bag-writeback` | npc_click + db_snapshot | DB/文件后态或逐字节未钉值 |
| 7 | `openmir2-compare-ident53-diff-triage` | spell_cast（复拍 + pin 转发链） | DB/文件后态或逐字节未钉值 |
| 8 | `openmir2-db-encoding` | login_only + db_snapshot | DB/文件后态或逐字节未钉值 |
| 9 | `openmir2-delmagic-addmagic-chain` | bag_equip | 需坐标/实体解码或尚无钉值 |
| 10 | `openmir2-encoding-core-e1-e2` | npc_click / merchant_dialog | 字节值未钉（无可断言字面） |
| 11 | `openmir2-gamesvr-serverinfo-report` | login_only | 字节值未钉（无可断言字面） |
| 12 | `openmir2-gm-impl-shutup` | login_only + 手动 @禁言 | 需坐标/实体解码或尚无钉值 |
| 13 | `openmir2-gm-script-host-seam` | login_only + 手动 @禁言 <名> 1 | 需坐标/实体解码或尚无钉值 |
| 14 | `openmir2-guard-ctor-constants` | login_only | 需坐标/实体解码或尚无钉值 |
| 15 | `openmir2-guild-g2-dispatch` | login_only | 已进判据表（机判） |
| 16 | `openmir2-healthspell-emission-model` | 新：R5 spell_cast | 场景未建（新场景/新夹具） |
| 17 | `openmir2-login-529-status-text` | login_only | 已进判据表（机判） |
| 18 | `openmir2-login-654-order` | login_only | 需坐标/实体解码或尚无钉值 |
| 19 | `openmir2-login-charstatus-657` | login_only | 已进判据表（机判） |
| 20 | `openmir2-login-frame-order-52-752` | login_only | 已进判据表（机判） |
| 21 | `openmir2-login-numeric-gaps` | login_only（首杀项另需 combat_hit） | 需坐标/实体解码或尚无钉值 |
| 22 | `openmir2-m3-party-attribution` | run_party_trade_pair.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 23 | `openmir2-m3-party-gainexp-npe` | run_party_trade_pair.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 24 | `openmir2-m3-pet-slave-clone46` | run_hero_pet_pair.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 25 | `openmir2-m3-pet-slave-lifecycle` | run_hero_pet_pair.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 26 | `openmir2-m3-pet-slave-masterdie-wire` | run_hero_pet_pair.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 27 | `openmir2-m3-rng-scene-driver` | run_rng_scene_pair.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 28 | `openmir2-m3-shop-dialog-wiring` | run_npc_shop_pair.ps1 | pair 夹具（run_*.ps1 输出，非帧证据） |
| 29 | `openmir2-market-def-impl` | merchant_dialog | 需坐标/实体解码或尚无钉值 |
| 30 | `openmir2-mb8-9-monsters` | 新：R5 spell_cast race 215 | 场景未建（新场景/新夹具） |
| 31 | `openmir2-menu-ok-unify-pickwire` | dice_chain_auto | 已进判据表（机判） |
| 32 | `openmir2-mp-session-projection-clobber` | spell_cast | DB/文件后态或逐字节未钉值 |
| 33 | `openmir2-npc-click-frame-seq-fix` | npc_click / npc_click_auto | 需坐标/实体解码或尚无钉值 |
| 34 | `openmir2-npc-ctor-constants` | login_only | 机判受限（series 需 word 选择器） |
| 35 | `openmir2-npc-display-table` | login_only + npc_click | 需坐标/实体解码或尚无钉值 |
| 36 | `openmir2-outbound-encoders-red-batch` | 新：R2 upgrade_retrieve | 场景未建（新场景/新夹具） |
| 37 | `openmir2-script-playdice-openybdeal` | dice_chain | 需坐标/实体解码或尚无钉值 |
| 38 | `openmir2-script-take-persist` | dice_chain + db_snapshot | DB/文件后态或逐字节未钉值 |
| 39 | `openmir2-searchtick-origin-parity` | login_only（--dwell-ms ≥2000） | 需坐标/实体解码或尚无钉值 |
| 40 | `openmir2-spell-fill-projection-mirror` | spell_cast + eat_item | 需坐标/实体解码或尚无钉值 |
| 41 | `openmir2-t138-request-castle-war` | king_castle_war | 需坐标/实体解码或尚无钉值 |
| 42 | `openmir2-transmap-maptrigger` | move_walk | 需坐标/实体解码或尚无钉值 |
| 43 | `openmir2-unbindlist-consumer` | eat_item | 需坐标/实体解码或尚无钉值 |
| 44 | `openmir2-upgrade-npc` | 新：R2 upgrade_retrieve | 场景未建（新场景/新夹具） |
| 45 | `openmir2-verify-tick-base` | login_only | 需坐标/实体解码或尚无钉值 |
| 46 | `openmir2-weapon-upgrade-values` | 新：R2 upgrade_retrieve | 场景未建（新场景/新夹具） |
| 47 | `openmir2-window-collector-preflight` | card_window.py 真栈一次（login_only） | 需坐标/实体解码或尚无钉值 |
| 48 | `openmir2-world-tick-expansion` | login_only + MIR2_RNG_LOG | 需坐标/实体解码或尚无钉值 |
| 49 | `openmir2-ya52-menu-ok-missing` | dice_chain_auto | 已进判据表（机判） |

## 7. 真机证据根实跑（现成证据，**未开服务端、未占端口**）

命令：

```sh
python -I -B tools/window/card_evidence.py --run-dir E:/tmp/window/run2/cs-probe \
    --json E:/tmp/card-evidence-run2-cs.json > E:/tmp/card-evidence-run2-cs.txt 2>&1
echo $?   # => 0
```

输出（全文见 `E:/tmp/card-evidence-run2-cs.txt`，共 27 行判据 + 1 行总计）：

```text
证据根场景 = login_only（来源 run.json:login_only）
...
PASS         openmir2-login-charstatus-657 A-class   ident=657 ×2 断言全部成立
PASS         openmir2-login-529-status-text A-class   ident=529 ×1 断言全部成立
总计：27 条 = PASS 2 / FAIL 0 / UNDECIDABLE 25（证据根 E:/tmp/window/run2/cs-probe，29 帧）
```

**判读（不许往大里说）**：

- 该根是 `login_only` 登录证据（`run.json:scenario.name`），所以只有「登录腿」的两条 A 类判据 PASS：
  `SM_CHARSTATUSCHANGED(657)` 恰 2 条、`SM_PASSOK_SELECTSERVER(529)` ≥1 条。
  这两条是**结构存在性**判据，**不是** 657 邻居相对序 / 529 逐字节文本的窗口收口结论。
- 其余 25 条全是 UNDECIDABLE，且原因可核：场景不符（`castle_*`/`dice_chain_auto`/`getbackupgnow`/`dropgold_zero_draw`）、
  人工判据（台账未给钉值/需 DB·文件·时钟）、`order` ident 未齐（52 有、621/211 无）、ident 未出现。
- `FAIL 0` 是**诚实结果**，不是「都过了」：本根根本不含 650/653/30/767 等帧，任何把它们判成 PASS 的实现都是错的。

| 文件 | sha256 |
|---|---|
| `E:/tmp/window/run2/cs-probe/frames.ndjson`（29 帧，证据） | `1a5fd75f6e6eafbd494d0cb6f26c6df4851733626262c3821922fb3cec134d0f` |
| `E:/tmp/window/run2/cs-probe/run.json`（场景归属来源） | `6b17f39bf25ec753615b96779b3b6d4e810c7351283abda863249d98b0abe091` |
| `tools/window/card-criteria.json`（判据表） | `044ac164c365c9ec2b76f6d5e73deae4ac221327b27bc377c2557c89e2e79585` |
| `tools/window/card_evidence.py`（运行器） | `9970e9d9bf59e8e83011d7155f18487fb7a18c82225f8012622aa7e2e2ad0d94` |
| `E:/tmp/card-evidence-run2-cs.json`（逐条结果） | `0943892c2189873e6fe93798a62ff26cfe45884f565698a3130d388378b0a8f3` |
| `E:/tmp/card-evidence-run2-cs.txt`（stdout 全文） | `22be4e0c569b336ce2663829bd415d142af80d87b8dcf4becd93d3e5c22621a6` |

另一根 `E:/tmp/scriptcov/t118/autoA-main`（`run.json` 场景 = `t118auto`）同命令跑过：27 条 = PASS 0 / FAIL 0 / UNDECIDABLE 27，
rc=0——因为判据表里没有 `t118auto` 场景的行，全部「场景不符」。这正是场景归属校验该有的行为（不张冠李戴）。

## 8. 牙：4 条变异「绿 → 红 → 还原绿」

`--selftest` 自带 14 项断言（含 4 条牙）。为证明牙**真的会咬人**，把实现逐条打坏再跑（每次跑完从 HEAD 还原）：

```sh
python -I -B tools/window/card_evidence.py --selftest                     # rc=0  GREEN 14/14
# T1 cmp 恒真 → 牙a 红；T2 缺 ident 判 PASS → 牙b 红；T3 空值不再 UNDECIDABLE → 牙c 红；T4 掩码失效 → 牙d2 红
```

实测输出：

```text
T1 期望值比较恒真 | rc=1 | 红牙行=['FAIL 牙a：期望值改错 ⇒ FAIL']
   还原后 | rc=0 | GREEN: card_evidence selftest 14/14
T2 缺 ident 当 PASS | rc=1 | 红牙行=['FAIL 牙b：ident 不在帧里 ⇒ UNDECIDABLE']
   还原后 | rc=0 | GREEN: card_evidence selftest 14/14
T3 空值不再 UNDECIDABLE | rc=1 | 红牙行=['FAIL 牙c：字段为空串 ⇒ UNDECIDABLE']
   还原后 | rc=0 | GREEN: card_evidence selftest 14/14
T4 掩码失效 | rc=1 | 红牙行=['FAIL 牙d2：同一条加 datetime mask ⇒ PASS（掩码生效）']
   还原后 | rc=0 | GREEN: card_evidence selftest 14/14
```

变异结束后 `git status` 干净（每次从 HEAD 还原后比对一致）；变异脚本为一次性工具，未入库。

## 9. 仍不可机判的卡与「缺什么」（下一步输入）

| 缺口类别 | 影响的 W 项 / 卡 | 缺什么（补上即可机判） |
|---|---|---|
| 钉值缺失 | W-PD-1（5008 Param/Tag）、W-CO-1（`@main` 说词）、A 类「字节未钉」2 张 | 在卡线程里把 oracle 字面值钉进判据表（**从 C# 代码取，不从实得帧取**） |
| 场景/夹具缺失 | W-CM-1、W-DG-2、W-T127-1、W-CO-1/6、A 类「场景未建」7 张 | 建 `castle_zone`/castle fixture、补 butch 击杀步、`t118` 同 seed 注入 |
| 时钟不可注入 | W-TAX-4、W-CO-6 | C# 侧 `MIR2_CLOCK_*`（与 `window-kit-coverage` P0 同源） |
| 解码器缺失 | W-CM-2 的实体/AOI 坐标、W-DG-1 的金币净差 | 帧尾按 oracle 口径解码 / DB-assets 快照读法 |
| 分组语义缺失 | W-MQ-2（同标签 3 帧出队序） | runner 增加 `tag`+负载分组断言 |
| word 选择器缺失 | `npc-ctor-constants`（series 低=4 高=2） | runner 增加 `field:word:low|high` 断言 |
| 非帧证据 | A 类「pair 夹具」8 张、DB 后态 5 张 | 走 `run_*.ps1` 输出 / DB 快照比对，本工具不覆盖（见 §4 分工） |

## 10. 残留物清单

- 入库：`tools/window/card_evidence.py`、`tools/window/card-criteria.json`、本文件。
- 未入库（临时物，路径登记于此）：`E:/tmp/card-evidence-run2-cs.{txt,json}`（本次实跑输出，供审查复现）、
  `E:/tmp/a-class-table.md`（§6 表的生成中间物）、`E:/tmp/patch_ce*.py`、`E:/tmp/mutate_ce.py`（一次性补丁/变异脚本）。
- 未触碰：`tools/conformance/card_window.py`、`tools/window/run_batch.sh`（tester 正在改）；
  未起服务端、未占端口、未动 `E:/MirServer`、未动 mysqld。
