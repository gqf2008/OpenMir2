# evidence-m3-drop-exp-purity-gate-20261013

卡 `openmir2-m3-drop-exp-align`（M3④ 掉落/经验）——**whitelist T-35 场景纯度门**离线交付证据。

> 缘由：owner 对 T-35 的**有条件批准**（线程 comment `c78bd13f`，2026-10-13）：新隔离口径必须
> 「除被测量路径（1 只 pin 怪 + 击杀→掉落/经验取数）外 RNG 消费者数 = 0」+ 逐消费者清单/计数 +
> **纯度门**（fail-closed + 改坏必红）+ **处理墙钟消费者**（`AnimalObject.Wondering`；本夹具取
> (a) pin 抑制）；**不许**拿「离刷怪圈远 / 图里 MonGen 零行 / 只 spawn 一只」单独当充分条件。

## 结论（本片可判部分）

- **纯度门离线就绪且 fail-closed**：`drop_exp_purity_gate.py selftest` **16/16 GREEN**（含
  空流水红 / 缺 site 红 / 墙钟红 / 未分类红 / 表歧义红 / 缺 measured 行 PREREQ / 表被改坏必红）；
  `run_drop_exp_pair.ps1 -SelfCheckNoStack` **12/12 GREEN**（原 7/7 + 门 5 项）。
- **改坏必红三条**（`03`/`04`/`05`）：注入墙钟抽数 ⇒ `wallclock_consumer` rc=1；未分类站点 ⇒
  `unknown_consumer` rc=1；删掉表里的 `measured` 行 ⇒ PREREQ rc=2（**表是承重的**，不允许
  「流水干净就放行」）。
- **正样本**（`06`/`07`）：只含 `MonGetRandomItems` 掉落取数 ⇒ GREEN + 逐消费者计数（`measured=2`，
  其余 0）。
- **Rust 侧 fail-closed**（`08`）：消费者表尚无 Rust `measured` 行（④-D 未落地）⇒ **PREREQ 拒跑**，
  不因「流水看起来干净」给假绿。
- 四门禁（`09`–`12`）：fmt 0 / clippy 0 / **92 suites 832 passed 0 failed 19 ignored** / msgcodegen 604。

## 交付物（本片，均在 m3bag 写集）

- `drop-exp/drop_exp_purity_gate.py`（新）——`gate` / `selftest`。
- `drop-exp/drop_exp_consumers.tsv`（新）——**唯一真源**：`side/class/pattern/max/oracle_ref/note`；
  class = `measured`/`setup`/`wallclock`/`other`。
- `drop-exp/run_drop_exp_pair.ps1`——新增 `-PurityOnly`、`-PuritySide/-PurityStream/-PurityRaw/-PurityTable/-PurityJson`
  与无栈自检 ⑧–⑫。
- `drop-exp/drop_exp_scenario.json`——新增 `purity` 段（口径/墙钟策略/表/流件/schema）。
- `drop-exp/README.md`、`tests/parity/whitelist.md`（T-34 自检数 7/7→12/12、T-35 正文更新）。

## 口径与边界（如实登记）

- **未做**（窗口期工作，需一侧栈 + BotSrv 掉落/经验驱动）：真机 100 杀两侧全场景流水过门。
  表里**只填**已由 oracle 源码 + 真机实测确认的 C# 行（`MonGetRandomItems` = measured；
  `Wondering`/`RegenMonsters`/`Merchant.Run` = 墙钟须 0）。场景 spawn/构造抽数（`CreateMonster@WorldServer.MonGen.cs:740`、
  `BaseObject..ctor@BaseObject.cs:438` 等）与 **Rust 侧站点**留到窗口期按**实测清单**分类后补入——
  补之前它们是 `unknown` ⇒ 门判红；**不许**为了拿绿放宽分类（否则就是 owner 禁止的「橡皮图章」）。
- **墙钟策略 (a) 已选**：(b)「接专用流」要落实现卡，门对 `--wallclock-policy dedicated` 暂 PREREQ 拒跑。
- **EOL/口径**：清单按**仓库字节（git blob / LF 归一）**记；`eolcheck` GREEN（`13`）。`repro/` 里是
  可复跑的正/负样本输入。
- **残留物**：本片仅新增本证据目录（+ 上述 6 个文件改动）；未起栈/未占端口；`E:/tmp/m3bag-*` 为本次
  门禁/复核临时物（未入库，可删）。
