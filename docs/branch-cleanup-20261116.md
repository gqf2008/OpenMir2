# 远端分支清理清单（2026-11-16）

> 范围：`walgit` 远端上的 `refs/heads/card/*`。**删除的是分支指针**；提交本身已从 `master` 可达，
> 回滚 = 把对应 sha 重新推回 `refs/heads/card/<名>`（`git push walgit <sha>:refs/heads/card/<名>`）。
> 授权：项目总监 2026-11-16 批准（四条：逐个核 → 清单留痕 → 排除在飞 → 窗口结束后执行）。
> **只删下列 34 个**；在飞的 3 个见 §3，不删。

## 1. 判定方法（逐个核，不靠记忆）

```bash
for ref in $(git branch -r | sed 's/^ *//' | grep '^walgit/card/'); do
  sha=$(git rev-parse "$ref")
  git merge-base --is-ancestor "$sha" master && echo "MERGED $ref $sha" || echo "UNMERGED $ref $sha"
done
```

判定口径：`git merge-base --is-ancestor <sha> master` 为真 ⇒ 已并入 ⇒ 可删。
**注意**：不要用 `grep -c MERGED` 计数——`UNMERGED` 含子串 `MERGED`，会多数；用 `awk -F'	' '$3=="MERGED"'`。

结果：`card/*` 共 **37** 条 ⇒ **34 条已并入**（§2 删除）、**3 条未并入**（§3 保留）。
**合计 34+3=37；不要删 37 个**——那会把 3 个在飞的也删掉。

## 2. 删除目标（34）

| # | ref | sha（删除前核） |
| --- | --- | --- |
| 1 | `walgit/card/1000-prereq-inventory` | `88ca0178c19b29c7e16a44fb21a0f7632374419b` |
| 2 | `walgit/card/binary-stamp` | `2ddbcc76b0962e3bf18e91b1918f69b8c3238e34` |
| 3 | `walgit/card/botsrv-actor-pool-leak` | `a85aca2aeca1187ea0536aaee5f93a70d958b280` |
| 4 | `walgit/card/botsrv-autoplay-nre` | `a720f852a9025bf56ffc6f3513283e73ce9dd3eb` |
| 5 | `walgit/card/botsrv-decode-compat` | `260508efccce52628389158f2a6be2c3d264713c` |
| 6 | `walgit/card/botsrv-error-census` | `727a3d47c91fbfaf43d589c68758a110a02e6828` |
| 7 | `walgit/card/botsrv-heap` | `7a44317603b6263969ee6c93a4c6673afeb41eab` |
| 8 | `walgit/card/botsrv-inworld-plateau` | `76d6173c0dba5f44ce9f623d66e91aae5e3259c2` |
| 9 | `walgit/card/botsrv-loh-growth` | `fef887eaaa075207b7dc0951215aef10db54e5b9` |
| 10 | `walgit/card/botsrv-memory-leak` | `93e46f56ea54879a7c40639fc239f77658d95024` |
| 11 | `walgit/card/botsrv-myself-wiring-tooth` | `335a7d9ed97239a76df8c7a1e6d8df3ef7756e55` |
| 12 | `walgit/card/botsrv-probe-turn-r2` | `8177684c7a16ee5f2c40b5cf43761a094160e9e8` |
| 13 | `walgit/card/botsrv-shared-state-locks` | `2d9297cbd5e2a28a625044267df5784fb7cd3273` |
| 14 | `walgit/card/distmap-design` | `6e476c0ed46cfb929e59006996b9454922561695` |
| 15 | `walgit/card/errorstack-diagnosable` | `3392ec912fa788d661a9ea6be8b6cb70e252b481` |
| 16 | `walgit/card/gate-backpressure-obs` | `1686cbd3010e7dddbe46dba25884902b4a3d0a52` |
| 17 | `walgit/card/gate-backpressure-queue` | `40e5e94d8d0bcfb0853966fdab56388ce307c4c2` |
| 18 | `walgit/card/gate-frame-criticality` | `0482f715daadecd232294bfc38422d84f54db454` |
| 19 | `walgit/card/host-segment-profiler` | `11f653d716bdf24ee324ebff558ec668a4d3b2bc` |
| 20 | `walgit/card/loadgate-criteria` | `3d98dfa7aa013dfcf920fa3f8237e8ca35e7b02f` |
| 21 | `walgit/card/loadgate-filehash-rc` | `5399eed7644bf6957b62e2bac8d66ec8ca9e273e` |
| 22 | `walgit/card/loginconns-buffer` | `156b57cf4f7c5653cf745a9450f9ec98445c2f2f` |
| 23 | `walgit/card/p1-2-hear-targets` | `b74c5425bd0a3bbf8d2c761f1e283923b4819829` |
| 24 | `walgit/card/p1-3-hear-probe` | `52bd5e6fc1f94d95186f795f95735b0e02991dd4` |
| 25 | `walgit/card/p1-hostinbound-index` | `e0135bb3d324414048872cdaf3f3ba526759da19` |
| 26 | `walgit/card/p2-1-shard-skeleton` | `eea7d203e543a36f2b501789201c2826f482d1d7` |
| 27 | `walgit/card/recv-null-fix` | `c205b172ab650105165554ce9ae53dae1ba28a3d` |
| 28 | `walgit/card/recv-null-fix-followup` | `6428ebe3cc722bce0a45f08bee6830413b03e3ad` |
| 29 | `walgit/card/recv-null-residual` | `4875c4d0642e3232c8bfbc87e1993e34928b069f` |
| 30 | `walgit/card/s5-stale-paths` | `07098f900f072951bdf9b148034243e161158146` |
| 31 | `walgit/card/simai-probe` | `a76958c9906df84fac78faa8f34e85fbe0d1f21f` |
| 32 | `walgit/card/smlogon-misdelivery` | `25146b80b76c2a38df374c77d11fc522e47a3bd5` |
| 33 | `walgit/card/time-field-audit` | `cd5103677d3c1595deb47e11572be9aff5e31d10` |
| 34 | `walgit/card/world-entry-serial-probe` | `4718c9cd55de9b173b5afc0b7d5ac9a11aa4af1e` |

## 3. 保留（3 个在飞，未并入 master）

| ref | sha | 理由 |
| --- | --- | --- |
| `walgit/card/botsrv-decode-window` | `f9fefe537174a366e9122c9eb3b9cc084258aac9` | 未并入 master，车道在用 |
| `walgit/card/botsrv-probe-turn` | `b47d005a59d03a1fd4ca6cded2509ec4f519cfd8` | 未并入 master，车道在用 |
| `walgit/card/p2-followups` | `5760efa9f070658dce0989e49a9ec9f3347f8367` | 未并入 master，车道在用 |

## 4. 执行与核验

```bash
# 删除（逐个，失败即停）
git push walgit --delete refs/heads/card/<名>
# 核验：剩余 card/* 应恰好是 §3 的 3 个
git ls-remote walgit 'refs/heads/card/*'
```

判据：删除后 `git ls-remote walgit 'refs/heads/card/*'` **只有 3 行**，且与 §3 的 sha 逐一相等；
`git rev-parse master` 与删除前相同（删远端 ref 不动本地 HEAD）。
