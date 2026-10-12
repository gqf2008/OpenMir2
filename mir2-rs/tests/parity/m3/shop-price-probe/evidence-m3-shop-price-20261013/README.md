# M3⑤-实现 3：商店价格 / 修理公式 —— 证据包

卡：`openmir2-m3-shop-price-formula`（issue 由 `openmir2-orchestrator` 建，owner_hint = `openmir2-worker-4` / m3bag）。
写集：`mir2-rs/crates/formula/**` + 必要向量；**零** `crates/world` / `bins` / `src` / `sql` / `MirClient`。

## 交付物

| 路径 | 说明 |
| --- | --- |
| `mir2-rs/crates/formula/src/shop.rs` | 新 shop 模块：`GetItemPrice`/`GetUserItemPrice`/`GetUserPrice`/`GetSellItemPrice` + 修理成本/副作用，逐条对照 oracle，含 `BUG-REPLICA` 标注 |
| `mir2-rs/crates/formula/tests/shop_price_vectors.rs` | 真身向量回读用例 + 2 条 redcheck |
| `mir2-rs/tests/parity/m3/shop-price-probe/Program.cs` | C# 真身探针：反射直调 `GameSrv.Npc.Merchant` 私有方法（DispatchProxy 替身 `IPlayerActor`/`IUserCastle`） |
| `mir2-rs/tests/parity/m3/shop-price-probe/vectors/shop_price_vectors.{json,tsv}` | 58 条真身向量（6 族） |
| 本目录 | 判据原始读数 |

## oracle 锚点（`src/`，grep -n 实测）

- `Merchant.GetItemPrice(ushort)` — `src/GameSrv/Npc/Merchant.cs:347`
- `Merchant.GetUserItemPrice(UserItem)` — `:1130`
- `Merchant.GetUserPrice(IPlayerActor, double)` — `:765`
- `Merchant.GetSellItemPrice(double)` — `:1358`（private static）
- `Merchant.ClientQueryRepairCost(IPlayerActor, UserItem)` — `:1555`
- `Merchant.ClientRepairItem(IPlayerActor, UserItem)` — `:1599`
- `HUtil32.Round`（`Math.Round(..., AwayFromZero)`）— `src/OpenMir2/HUtil32.cs:131`；`_MAX` — `:674`
- `SystemShare.Config` 默认值 — `SuperRepairPriceRate=3`/`RepairItemDecDura=30`/`CastleMemberPriceRate=80`（`GameSvrConf.cs:1709/1710/1796`）

## 照搬的 oracle 缺陷（`BUG-REPLICA`）

1. `Merchant.cs:1179` 守卫 `if (nC != 4 || nC != 9)` 逻辑或**恒真**（死代码）⇒ Rust 省略该判断、直接执行唯一可达分支（行为等价）。
2. `Merchant.cs:1158`：`StdMode == 43` 时 `GetUserItemPrice` **写回 `userItem.DuraMax = 10000`**（取价带副作用）。
3. `Merchant.cs:1206`：`itemPrice = itemPrice / 5 * n14` 是 **double 除法**（非 C# 整型除法）。
4. 一般分支用**模板** `stdItem.DuraMax` 折算到**实例** `userItem.DuraMax`（两个口径各用一次）。
5. `Merchant.cs:1656`：普通修理耗久 `(DuraMax - Dura) / RepairItemDecDura` 是 **C# 整型除法（向零截断）**。

## 判据读数（全部离线，零端口）

| # | 判据 | 读数 | 原始件 |
| --- | --- | --- | --- |
| ① | oracle 锚点逐条 | `Merchant.cs:347/1130/765/1358/1555/1599`、`HUtil32.cs:131/674`、默认配置 3/30/80 | `01-oracle-anchors.txt` |
| ② | 真身向量生成 | 58 条（`item_price` 6 / `uip` 15 / `user_price` 8 / `sell_price` 8 / `repair_cost` 10 / `repair_item` 11），探针 rc=0 | `02-probe-build-run.txt`、`03-vectors-inventory.txt` |
| ③ | Rust 逐族对拍 | `cargo test -p mir2-formula` **14 passed / 0 failed**（含 `shop_price_vectors_match_oracle` 258 处断言） | `04-rust-test-green.txt` |
| ④ | 改坏必红 ×3 | ① 去掉 `StdMode 43` DuraMax 副作用 ⇒ `stdmode43_clamp` got 300 vs 158；② 耗久改四舍五入 ⇒ `trunc_dec` dura 967 vs 968；③ `GetItemPrice` 命中 0 也回落 ⇒ `list_zero_no_fallback` got 321 vs 0 | `05/06/07-mutation-red-*.txt` |
| ⑤ | 四门禁（隔离 target / `--offline`） | fmt 0 / clippy 0 / `test --workspace --no-fail-fast` **87 suites / 781 passed / 0 failed / 18 ignored** / msgcodegen 604 rc=0（合并 origin/master a547619a 后的合并树） | `08-gates.txt` |
| ⑥ | 干净检出可复现 | 见 `09-clean-checkout-repro.txt` | `09-clean-checkout-repro.txt` |

> 合并树 `781 passed`；其中本片新增 3（集成用例）⇒ 本片相对 master（778）为 `+3`，suite 数不变（新增 test 落在既有 suite 文件之外的新集成 target ⇒ `87 = 86 + 1`）。

## 复现命令（离线，零端口）

```sh
# 1) 重新生成真身向量（需 .NET SDK；在仓库根运行）
dotnet build mir2-rs/tests/parity/m3/shop-price-probe/shop-price-probe.csproj -c Release
dotnet mir2-rs/tests/parity/m3/shop-price-probe/bin/Release/net8.0/shop-price-probe.dll \
  mir2-rs/tests/parity/m3/shop-price-probe/vectors/shop_price_vectors.json
# 2) Rust 侧对拍
cd mir2-rs && CARGO_TARGET_DIR=E:/tmp/m3bag-shop-target cargo test -p mir2-formula --offline
# 3) 清单复核（仓库 LF/git-blob 口径）
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/shop-price-probe/evidence-m3-shop-price-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck \
  mir2-rs/tests/parity/m3/shop-price-probe/evidence-m3-shop-price-20261013
```

## 口径与残留

- 本包不在仓库内新增/修改表结构、脚本语法、数据格式；`src/**`、`sql/**`、`MirClient` 零改动（oracle 只读引用）。
- `dotnet build` 会在 `src/*/bin|obj`、`mir2-rs/tests/parity/m3/shop-price-probe/bin|obj` 落构建产物（均被 `.gitignore` 覆盖，不入库）。
- 未起任何服务端栈、未占端口。
- 本片未实现接线（8 条 `CM_*` 派发 / `Merchant` 实体 tick / 落库），按盘点 §3 属 `openmir2-m3-shop-dialog-wiring` 等后续卡；本卡只交付公式面 + 真身向量。
