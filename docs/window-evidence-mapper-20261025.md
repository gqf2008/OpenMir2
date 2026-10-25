# 窗口证据映射器（card evidence mapper）— 2026-10-25

- 卡：`openmir2-window-evidence-mapper`（tier **L**，owner `openmir2-dev-2`，branch `card/window-evidence-mapper`）。
- 定位：把条件② 的「卡面判据」从散文变成**机器可判**：一张判据表（`tools/window/card-criteria.json`）+ 一个只读运行器
  （`tools/window/card_evidence.py`），对**已采到的帧证据根**逐条给出 `PASS / FAIL / UNDECIDABLE`。
- 红线：**UNDECIDABLE 绝不是 PASS**；缺数据/缺字段/空值一律不通过；不按实得帧回填期望值；
  运行器**只读**（不起服务端、不占端口、不写证据根）。本卡不判 `closed`，只交付工具与判据表。

> **红线（同步点 ≠ 成功判据）**：审「某帧出现 = 成功」时，必须先到 oracle 看该帧是在**成功分支**里发，
> 还是在**函数尾部/无条件路径**里发——例如 `SM_GAMEGOLDNAME(5008)` 每次登录必发
> （`PlayObject.Message.cs:1689` RM_LOGON）、`SM_MENU_OK(767)` 全仓 28 处多数是 `[错误]` 弹窗、
> `SM_BUTCH(637)` 是函数尾部无条件回帧。这类 ident 的机判条目**必须带 `fields` 或 `order`**，
> 否则判据表不合法（运行器 rc=2，卡 `openmir2-criteria-table-hardening` 的结构性守卫）。

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
        "min_count": 2,                // 可选：该 ident 至少出现几次（不足 ⇒ UNDECIDABLE）
        "max_count": 2,                // 可选：至多几次（超出 ⇒ FAIL，用来表达「恰一条」）
        "count_scope": "all",          // 可选：filtered（默认，按 recog 过滤后计数）/ all（按 ident 总帧数）
        "match": "last",               // 可选：last（默认，只判最后一帧）/ any / all
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
      "narrowed": "相对 source 收窄了什么",   // 可选；machine=true 但只覆盖 source 的一部分时必须写
      "machine": false,                // 本轮不可机判：必须带 reason（否则表不合法，rc=2）
      "reason": "缺什么 / 为什么"       // 运行器输出 UNDECIDABLE + 该原因，**不许静默跳过**
    }
  ]
}
```

- `cmp ∈ {eq, ne, ge, le, contains, regex}`；`mask ∈ {datetime, digits, none}`。
  `datetime` 把 `YYYY-MM-DD[ HH:MM:SS]` 抹成 `<DATETIME>` 再比（`$DATETIME` 类字段必须用它）；`digits` 抹掉所有数字串。
- 计数：先按 ident（+ `only_when`）取帧，再按 `recog`（若声明）过滤；`min_count` 不足 ⇒ **UNDECIDABLE**，
  `max_count` 超出 ⇒ **FAIL**（多出来的帧是回归，不是缺数据）；`recog` 不命中仍按原语义判 **FAIL**。
- **计数口径 `count_scope`**（卡 `openmir2-card-evidence-count-scope`）：
  `filtered`（默认）= 只数 `recog` 过滤后的帧；`all` = 数该 ident 的**总帧数**（不受 recog 过滤影响）。
  典型用途：台账 W-DG-1 的「653 **一条**」——若另有 `recog=999` 的 653，`filtered` 会放过，`all` 判 FAIL。
  当前判据表里：**只有 W-DG-1 用 `all`**；其余带计数的条目（W-TAX-1、W-PD-1 的 5008、A 类 657×2/529/767×2、
  W-DG-2 的 637）都是 `filtered`（即默认）。
- 字段比较默认取**最后一帧**（`match: last`）；`match: any` = 任一帧满足即 PASS；`match: all` = 每帧都必须满足。
  「最后一帧」是**钉死的默认**，不再由调用者脑补。
- `narrowed`：machine=true 但只覆盖 source 的一部分时**必须**写明收窄了什么（审查据此判断判据是否够）。
- 表不合法（缺字段、未知 `cmp`/`mask`、`machine:false` 无 `reason`）⇒ 运行器 **rc=2**（fail-closed，不会退化成「全 UNDECIDABLE」）。

## 2. 运行器用法与退出码

```sh
python -I -B tools/window/card_evidence.py --run-dir <证据根> [--scenario <名>] [--allow-unverified] \
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
- 两者皆无 ⇒ **全部 UNDECIDABLE**（fail-closed）；确需放行必须显式加 `--allow-unverified`，
  此时结果标 `provenance: unverified`，**不得当窗口证据**；
- 判据的 `scenario` 与证据根不一致 ⇒ 该条 **UNDECIDABLE「场景不符」**（不是 PASS，也不是 FAIL）。

## 3. 三态语义（钉死）

| verdict | 触发条件 |
|---|---|
| PASS | 场景相符；`order` 成立；`min_count ≤ 次数 ≤ max_count`；`recog`（若声明）命中；按 `match` 语义，字段存在且比较成立 |
| FAIL | 场景相符、数据齐，但 `order` 不成立 / `recog` 不命中 / **次数超过 `max_count`** / 任一（按 `match`）字段比较不成立 |
| UNDECIDABLE | `machine:false`（人工判据）；场景不符；**证据根场景未声明且未放行**；ident 不在证据根；次数不足 `min_count`；字段缺失、`null`、空串；无法比较（如对非数字做 `ge`）；`order` 里任一 ident 缺 |

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
| 3 | `openmir2-script-playdice-openybdeal` | W-PD-1 | **机判（已收紧）**：`SM_GAMEGOLDNAME` **≥2 帧**（登录 + 兑换后 `GameGoldChanged`）且**最后一帧** `param=LoWord(GamePoint)=0` / `tag=HiWord(GamePoint)=0`（前置 GamePoint=9，兑换后 9→0）；登录必发帧（`:1689`）不再单独构成 PASS + 人工 1 条（DB 后态） | 只判「出现」会因每次登录必发而假绿（卡 `openmir2-criteria-table-hardening`） |
| 3 | 同上 | W-PD-2 | 人工 | 部署无 `@back`（非目标）；`@back2` 属部署内容差异 |
| 4 | `openmir2-clearmapmon-impl` | W-CM-2 | **人工（2 条）**：① Q001 判 30 帧与已合并文档**相反**，按 `exec-sheet:31` / `scenes doc:110-111`（master）改判人工；② 实体/AOI 差集与坐标 | 30 帧属 t118 清场腿（该场景永久不可达）；坐标/实体面需按 oracle 口径解码帧尾 |
| 4 | 同上 | W-CM-1 | 人工 | 场景 blocked（`t118` 的 `movr M3` 不可注入）⇒ 无证据可采 |
| 5 | `openmir2-m3-castle-siege-state` | W-T127-1 | 人工 | `castle_zone` fixture 未建；相对序需与文本/收件人绑定且未钉字面值 |
| 6 | `openmir2-inbound-dispatch-red-batch` | W-DG-1 | **机判**：`SM_GOLDCHANGED` **总帧数恰 1**（`count_scope=all`，`min_count=max_count=1`）、且 `recog=100`、`param=0`（scenario `dropgold_zero_draw`）+ 人工 1 条（金币净差/地面物件/RNG 账本） | 金币差与地面物件在 DB/地图侧；RNG 流位需 `MIR2_RNG_LOG` |
| 6 | 同上 | W-DG-2 | **机判（订正）**：`SM_DEATH(32)` → **RNG 账本恰两抽**（`max=16` ×1 @ `Operate.cs:722`、`max=201` ×1 @ `:723`）；`SM_BUTCH(637)` 降为**伴随证据**（仍在 `order` 里，不许删）；`SM_SKELETON(33)` 仅 `BodyLeathery<=0` 才发，是加强项而非必需（注意拼写是 `BodyLeathery`）。证据根无 `rng-stream.tsv` ⇒ UNDECIDABLE | 账本由 `--rng-log` 产出；场景 blocked 清理与进批归另一张卡 |
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
```

判据共 28 条：PASS 数 2、UNDECIDABLE 26、FAIL 数为 0（快照 @ 2026-10-25；证据根 `E:/tmp/window/run2/cs-probe`，29 帧）。

**判读（不许往大里说）**：

- 该根是 `login_only` 登录证据（`run.json:scenario.name`），所以只有「登录腿」的两条 A 类判据 PASS：
  `SM_CHARSTATUSCHANGED(657)` 恰 2 条、`SM_PASSOK_SELECTSERVER(529)` ≥1 条。
  这两条是**结构存在性**判据，**不是** 657 邻居相对序 / 529 逐字节文本的窗口收口结论。
- 其余 25 条全是 UNDECIDABLE，且原因可核：场景不符（`castle_*`/`dice_chain_auto`/`getbackupgnow`/`dropgold_zero_draw`）、
  人工判据（台账未给钉值/需 DB·文件·时钟）、`order` ident 未齐（52 有、621/211 无）、ident 未出现。
- **一条失败都没有**是诚实结果（快照 @ 2026-10-25），不是「都过了」：本根根本不含 650/653/30/767 等帧，任何把它们判成 PASS 的实现都是错的。

| 文件 | sha256 |
|---|---|
| `E:/tmp/window/run2/cs-probe/frames.ndjson`（29 帧，证据） | `1a5fd75f6e6eafbd494d0cb6f26c6df4851733626262c3821922fb3cec134d0f` |
| `E:/tmp/window/run2/cs-probe/run.json`（场景归属来源） | `6b17f39bf25ec753615b96779b3b6d4e810c7351283abda863249d98b0abe091` |
| `tools/window/card-criteria.json`（判据表） | `b48827dcf41ada572624e93e227f5c20bab406c122266424f51770e3a95cb037` |
| `tools/window/card_evidence.py`（运行器） | `cde21829c3537435c0b15b3e3558eab1fb164e2d97c84536f857b205230b5df6` |
| `E:/tmp/card-evidence-run2-cs.json`（逐条结果） | `804bb51cb3a50a844de05a32ca9bc1306ce0292dec7092582568f03f7db0504b` |
| `E:/tmp/card-evidence-run2-cs.txt`（stdout 全文） | `4fabc81d81b9ea04a7836618598b6fef49adee82b9b9fd4140856ffa4aca9312` |

另一根 `E:/tmp/scriptcov/t118/autoA-main`（`run.json` 场景 = `t118auto`）同命令跑过：判据 28 条，PASS 数为 0、UNDECIDABLE 28 条、失败 0 条（快照 @ 2026-10-25），
rc=0——因为判据表里没有 `t118auto` 场景的行，全部「场景不符」。这正是场景归属校验该有的行为（不张冠李戴）。

## 8. 牙：9 条变异「绿 → 红 → 还原绿」

`--selftest` 共 **23 项**断言（A–J 十组牙 + schema 负例）。为证明牙真的会咬人，把实现逐条打坏再跑
（每次跑完从 HEAD 还原，并确认再次 `GREEN 23/23`）：

| 变异 | 咬哪颗牙（红行） |
|---|---|
| M1 `cmp=='eq'` 恒真 | `牙a：期望值改错 ⇒ FAIL` |
| M2 缺 ident 判 PASS | `牙b：ident 不在帧里 ⇒ UNDECIDABLE` |
| M3 空串不再 UNDECIDABLE | `牙c：字段为空串 ⇒ UNDECIDABLE` |
| M4 掩码失效 | `牙d2：同一条加 datetime mask ⇒ PASS（掩码生效）` |
| M5 关掉场景归属 | `牙h：证据根场景 A ≠ 判据场景 B ⇒ UNDECIDABLE` |
| M6 关掉 `min_count` | `牙g：次数 < min_count ⇒ UNDECIDABLE` |
| M7 关掉 `max_count` | `牙f：帧数超 max_count ⇒ FAIL（恰一条）` |
| M8 关掉「未验归属」闸门 | `牙j：缺 run.json + 无 --scenario ⇒ 全 UNDECIDABLE` |
| M9 `match=any` 退化 | `牙i-对照：match=any 有帧满足 ⇒ PASS` |

实现里没有「看起来对但无人守」的核心谓词：三态判定、场景归属、计数上下界、`match` 语义、掩码、
未验证据闸门都有对应的红行。变异脚本为一次性工具，未入库。

## 9. 仍不可机判的卡与「缺什么」（下一步输入）

| 缺口类别 | 影响的 W 项 / 卡 | 缺什么（补上即可机判） |
|---|---|---|
| 钉值缺失 | W-PD-1（5008 Param/Tag）、W-CO-1（`@main` 说词）、A 类「字节未钉」2 张 | 在卡线程里把 oracle 字面值钉进判据表（**从 C# 代码取，不从实得帧取**） |
| 场景/夹具缺失 | W-CM-1、W-T127-1、W-CO-1/6、A 类「场景未建」7 张 | 建 `castle_zone`/castle fixture、`t118` 同 seed 注入 |
| 时钟不可注入 | W-TAX-4、W-CO-6 | C# 侧 `MIR2_CLOCK_*`（与 `window-kit-coverage` P0 同源） |
| 解码器缺失 | W-CM-2 的实体/AOI 坐标、W-DG-1 的金币净差 | 帧尾按 oracle 口径解码 / DB-assets 快照读法 |
| 分组语义缺失 | W-MQ-2（同标签 3 帧出队序） | runner 增加 `tag`+负载分组断言 |
| word 选择器缺失 | `npc-ctor-constants`（series 低=4 高=2） | runner 增加 `field:word:low|high` 断言 |
| 非帧证据 | A 类「pair 夹具」8 张、DB 后态 5 张 | 走 `run_*.ps1` 输出 / DB 快照比对，本工具不覆盖（见 §4 分工） |
| 30 帧判据 | W-CM-2 的 Q001 腿 | **已改人工项**（与已合并文档相反，见 §10 F1），不是缺口而是口径订正 |

## 10. 复审修复记录（reviewer-21 `b663dae5` request_changes + reviewer-20 `3da0792a` info）

按「判据对不对 > 工具跑得干净」重审了一遍**每条 machine=true 的判据**，逐条对已合并文档原文（master，不是台账摘要）：

| 项 | 处置 | 落点 |
|---|---|---|
| **F1**（判据错） | W-CM-2 的 `clearmapmon_q001` **不再判 `SM_DISAPPEAR(30)`**：`docs/window-exec-sheet-20261025.md:31` 已把 #19 收窄为「只验刷怪」，`docs/window-scenes-clearmapmon-20261025.md:110-111` 明写「不会有 `SM_DISAPPEAR(30)`」；30 帧属 t118 清场腿（`exec-sheet:47` 已定性永久不可达）⇒ 改 `machine:false + reason`，报告 §5 第 4 行同步 | `card-criteria.json`、本文 §5 |
| **F2**（表达力） | schema 加 `max_count`（超出 ⇒ FAIL）；W-DG-1 改 `min_count=1, max_count=1`（台账「653 **一条**」，`exec-sheet:32` 逐字） | `card_evidence.py`、判据表、牙 f + M7 |
| **F3**（缺牙） | 场景归属补牙：`run.json=A`、判据=`B` ⇒ UNDECIDABLE（牙 h）；关掉该行 ⇒ 红（M5） | 牙 h、M5 |
| F4（minor） | 4 条比 source 弱的判据加 `narrowed` 逐条写明收窄：657（只判次数、邻居相对序未判）、529（只判出现、字节未判）、52/621/211（只判相对序、字段布局未判）、W-PD-1（只判 5008 出现，Param/Tag 与落库另条）、W-TAX-1（88/4 与文件面属 W-TAX-2）、W-MQ-1（用首帧 seq） | 判据表 `narrowed` |
| F5（minor） | `min_count` 补牙（牙 g）+ M6 | 牙 g、M6 |
| F6（info） | 同 ident 多帧语义钉死为 `match: last`（默认），可选 `any`/`all`；schema 与本文同步（牙 i + M9） | 牙 i、M9 |
| F7（info） | 证据根无 `run.json` 且无 `--scenario` ⇒ **全部 UNDECIDABLE**；`--allow-unverified` 才放行并标 `provenance=unverified`（牙 j + M8） | `main`、牙 j、M8 |
| reviewer-20 info | A 类 7 条的 `w_item` 从同一个 `A-class` 改成可区分的 `plan-§2:<卡/序>` | 判据表 |
| **N1/I1（本卡）** | 计数口径歧义：`min/max_count` 原来只数 `recog` 过滤后的帧 ⇒「653 **总帧数**恰 1」不可表达（两帧 653、recog 100/999 仍 PASS）。新增 `count_scope: filtered(默认)/all`；W-DG-1 改 `all`；非法值加载期拒（牙 k/k2/k 对照/m + 变异 C1/C2） | `card_evidence.py`、判据表 |
| **W-DG-2 再订正（`openmir2-criteria-table-hardening`）** | 主判据搬到**账本恰两抽**（`max=16`×1 @ `Operate.cs:722`、`max=201`×1 @ `:723`）；637 降伴随证据（仍在 order，不许删）；33 为 `BodyLeathery<=0` 加强项。无账本 ⇒ UNDECIDABLE；`card-criteria.json` 同条已同步 | 判据表、本文 §5 |

**machine=true 判据复核结论**（对 master 文档原文）：W-TAX-1（`exec-sheet:34` 的 `650.recog = Gold 9912`）✓；
W-DG-1（`exec-sheet:32` 的「恰一条」⇒ 已加 max_count）✓；W-MQ-1（`docs/script-handler-msgqueue-20261025.md:73`
的 643→767→44→645→5008）✓；W-PD-1（出现性判据，条件门 `SoftVersionDateEx != 0` 只会让它缺席 ⇒ 不假 PASS，已加 `narrowed`）✓；
A 类 6 条按 §6 的选取规则，均已加 `narrowed` 说明覆盖范围。

修复后 selftest **29/29 绿**，真机根复跑 28 条 = PASS 2 / UNDECIDABLE 26 / 失败 0（新增的两条是真机根缺帧 ⇒ UNDECIDABLE，**不是** PASS）。

## 11. 残留物清单

- 入库：`tools/window/card_evidence.py`、`tools/window/card-criteria.json`、本文件。
- 未入库（临时物，路径登记于此）：`E:/tmp/card-evidence-run2-cs.{txt,json}`（本次实跑输出，供审查复现）、
  `E:/tmp/a-class-table.md`（§6 表的生成中间物）、`E:/tmp/patch_ce*.py`、`E:/tmp/mutate_ce.py`（一次性补丁/变异脚本）。
- 未触碰：`tools/conformance/card_window.py`、`tools/window/run_batch.sh`（tester 正在改）；
  未起服务端、未占端口、未动 `E:/MirServer`、未动 mysqld。
