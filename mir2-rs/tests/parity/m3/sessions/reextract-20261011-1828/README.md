# reextract-20261011-1828 —— 真实 13 表快照上的「族表跨侧」重抽（F-1 / O-1 证据）

卡 `openmir2-m3-bag-equip-alignment`（round 2，reviewer-4 `cf067560` 的 F-1/O-1）。
本目录只用 `tools/dbsnap/dbsnap.py` + `scoped_diff.py` 读现网 `mir2_db`，**不起任何服务**
（不占 5000/7000/7100/7200）。

## 为什么要有这一份

- **F-1**：第一片把 `extract` 的 fail-closed 前移，却只登记了 13 张真实快照表里的 10 张
  （缺 `characters_item_attr` / `characters_magic` / `characters_quest`）⇒ `run_pair.ps1` /
  `check_db_paths.ps1` 在每条真实快照上 exit 4。修好后必须在**真实 13 表快照**上实跑一次 extract。
- **O-1**：旧 `cross_side_reuse` 用的 `scoped_after_norm.jsonl` 只含 4 张表
  （account / account_protection / characters / characters_indexes），**没覆盖**本片新增的族表。
  这里对既有会话前缀 `m3142347`(C#) / `m3143423`(Rust) 重跑 extract，让族表跨侧被覆盖。

## 快照（只留 manifest；TSV 体积大，按仓库惯例 `sessions/**/snap_*/` 已 gitignore）

```
python tools/dbsnap/dbsnap.py snap --out <snapdir>        # DEFAULT_TABLES = 13 张
# SNAP_OK 13 表
```

`snap_after.manifest.json` sha256 =
`ee8340d83cd3219c0ca2cf3811f0795f6f9186df75f5179c6493f21b2d2f166c`
（表清单与列名见 `sql/mir2_db.sql`；三张新登记表的 CREATE TABLE 在 `sql/mir2_db.sql:371883/371912/371937`）。

## 抽取（extract；F-1 的实跑段）

```
python scoped_diff.py extract <snapdir> m3142347 <out> --normalize
EXTRACT prefix=m3142347 rows=115 (family_by_playerid=111 player_ids=1)   rc=0
python scoped_diff.py extract <snapdir> m3143423 <out> --normalize
EXTRACT prefix=m3143423 rows=115 (family_by_playerid=111 player_ids=1)   rc=0
```

两表 13 张都**没**触发 fail-closed（修前会在 `characters_item_attr` 处 exit 4）。

入库件（本目录）：

| 文件 | 行数 | sha256 |
| --- | --- | --- |
| `scoped_m3142347_csharp.jsonl` | 115 | `5ad90c37a6b71f226c36946cc13bb5d049f53a98ffdb13007e9a940a49d1d321` |
| `scoped_m3143423_rust.jsonl` | 115 | `eb091747201541090b4d5d6a1800e58bbb17e39ce895ef5cd7b9a4ca2901bd46` |
| `scoped_m3142347_csharp_family.jsonl` | 111 | `8227adaa0ad6b8910047c547e9818001086052bcb4a9407a7a72b9b8d6c4b8de` |
| `scoped_m3143423_rust_family.jsonl` | 111 | `2ac5195107fcc735b42f255897ac8ab510985bf48df9d13a5dda5ac9cc795bbb` |

族表列（`*_family.jsonl`）= 全量里 `table.startswith("mir2_db.characters_")` 且
`!= "mir2_db.characters_indexes"` 的行（过滤命令见下），即 `characters_ablity` / `characters_item` /
`characters_bagitem` / `characters_storageitem` / `characters_status`。

## 跨侧比对

全量（含 account/characters/indexes）：

```
python scoped_diff.py diff <csharp>/scoped_m3142347_csharp.jsonl \
                           <rust>/scoped_m3143423_rust.jsonl \
                           --canonical --fixed-newchr
DIFF mir2_db.characters: rows A=1 B=1 仅A=1 仅B=1      # CX/CY（见下「已知观察」）
RED: 作用域内 2 行不一致                                  rc=1
```

**族表子集**（本片新增覆盖的作用域）：

```
python - <<'PY'
import json
for side in ('csharp','rust'):
    src=f'scoped_m3142347_csharp.jsonl' if side=='csharp' else 'scoped_m3143423_rust.jsonl'
    out=f'{side}-family.jsonl'
    with open(src,encoding='utf-8') as f, open(out,'w',encoding='utf-8',newline='\n') as g:
        for line in f:
            r=json.loads(line)
            if r['table'].startswith('mir2_db.characters_') and r['table']!='mir2_db.characters_indexes':
                g.write(line)
PY
python scoped_diff.py diff csharp-family.jsonl rust-family.jsonl --canonical --fixed-newchr
GREEN: 作用域内 5 张表逐行一致                          rc=0
```

⇒ **族表（5 张 / 111 行×2）跨侧字段级一致**；`MakeIndex` / `Position` / `StdIndex` / `Dura` /
`DuraMax` / Abil / Status 都在比，没有把任何被比对列挪进白名单。

## 已知观察（不是白名单，不藏）

全量比对只剩 `mir2_db.characters` 一行的 `CX/CY` 不一致：C# 侧 `(649,629)`、Rust 侧 `(650,630)`
（`HomeX/HomeY` 两侧都是 `650/631`）。这是**本次重抽快照（活库当前态）**才有的：历史上
`sessions/csharp-20261011-142347/scoped_after_norm.jsonl` 里该行 `CX=CY=0`（快照取于
**logout 落库前**，`LASTUPDATE` 14:24:13 vs 建角 14:24:08），所以旧跨侧 GREEN 没比到这一列。
两个会话的最终落库位置差 1 格，属**移动/落库时机**面，不在本卡「背包/装备」写集内；
已在卡内 comment 记为观察项，交 M3 门禁加固卡（`openmir2-m3-gate-script-hardening`）核。

## 复现

TSV 不入库，复现需重拍（MySQL 在跑即可，**不用起栈**）：

```
python tools/dbsnap/dbsnap.py snap --out <snapdir>
python scoped_diff.py extract <snapdir> m3142347 out_csharp.jsonl --normalize
python scoped_diff.py extract <snapdir> m3143423 out_rust.jsonl --normalize
python scoped_diff.py diff out_csharp.jsonl out_rust.jsonl --canonical --fixed-newchr
```

注：活库会继续被别的线改动 ⇒ 复现得到的 `characters` 行可能与上表不同（族表是稳定面）。
