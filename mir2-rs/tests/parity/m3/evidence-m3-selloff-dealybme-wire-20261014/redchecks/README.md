# redchecks - M3 10-1 收口片 #2 改坏必红

| # | 突变位置 | 突变 | 读数 |
| --- | --- | --- | --- |
| RC1 | bins/game-svr/src/main.rs | @dealybme 派发条件改 false &&（整块禁用）| RC1-dealybme-block-removed-mutated.txt rc=101 RED |
| RC1 | 还原 | 备份副本回填 | RC1-restored.txt rc=0 GREEN |
| RC2 | bins/game-svr/src/main.rs | 去掉 mflags.is_yb_deal 门（true &&）| RC2-isybdeal-gate-removed-mutated.txt rc=101 RED |
| RC3 | bins/game-svr/src/selloff.rs | merchant_say 的 series: 1 -> 0 | RC3-merchantsay-series-mutated.txt rc=101 RED（24 passed / 2 failed）|
| RC3 | 还原 | 备份副本回填 | RC3-restored.txt rc=0 GREEN |

还原后 git status --porcelain 为空（见 04-boundary.txt 末段）；还原用 cp 备份副本，未用 git checkout --。