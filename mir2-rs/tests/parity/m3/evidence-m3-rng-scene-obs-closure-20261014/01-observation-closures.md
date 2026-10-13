# M3 RNG 站点组卡：残余 observation 收口（2026-10-14）

- 卡：`openmir2-m3-rng-trace-scene-groups`。片：收口本卡**仍未收**的三条非阻塞 observation。
- 基线：上一片（`3f393eef2`，check-pack 接线）已由 reviewer-5 approve 并合入 master `6afa376363d3378933d95c26c45cd8eb7c9738f3`；
  本片 base = `6afa376363d3378933d95c26c45cd8eb7c9738f3`，head = `d4274cdaf0fa51f2d5fc946e77f371b260b8837a`。
  > 2026-10-14 base 前进重交：master 已前进 9 提交，为清除「回退假差量」把 base 前进到 `768b2bb6c4b81e774a80a2ad27fda0cccdb1a12c`
  > （merge head `8c38c0fcd3c20fca59fcc0055c86404a95b18e02`）；本片内容逐字节不变，净差量仍全在 `mir2-rs/tests/parity/m3/**`，详见 `13-base-advance.txt`。
- 写集：`mir2-rs/tests/parity/m3/**`，**零 `.rs`**；未起栈、未占端口、未 merge、未自签 review。

| # | 来源（review 条目 @ reviewed_head） | 问题 | 本片处置 |
| --- | --- | --- | --- |
| **A** | reviewer-5 `c3fef2fd278e` O-1（@ `c4dd70086`） | DoD② 报告 §0 状态行仍写 `head = `15e774e91` 之上的交付头`（更早批准片 `72c9258dc` 的旧文）⇒ doc-vs-事实；建议「对齐」或「把 head/base 纳入机判」 | ① 报告 §0 钉成 40 位全 oid（head `c4dd70086ea6801cd9c6b538671bffc6a657f98f`、base `c127ae9a43ce53697f430b99fcba989acd288211`）；② `check_evidence_counts.py` 新增 §0 `head/base` 全 oid fail-closed 机判（selftest **7→10**：短 oid 必红 / 全 oid 绿 / 仓内不可解析必红）；③ 接入 `run_rng_trace_pair -SelfCheckNoStack`（自检项数 **34→37**）+ 改坏必红（副本报告 head 改 7 位 ⇒ rc=1 点名 `declared_head`） |
| **B** | reviewer-6 `37c322a9e55c` M-1（@ `06f5faf7b`） | patch/status 称本片为「窗口前置审计 #3」，而新增代码注释写「#2」（`rng_consumer_groups.py` 段头 + `run_rng_trace_pair.ps1` ㉚） | 两处注释订正为 `#3`（与证据包名「第三次窗口前置审计」一致） |
| **C** | reviewer-5 `9dce8b2efb09` M-1（@ `a992a09a9`） | 证据 `10-mutation-a-gate-ignores-stream-seed.txt` 记「恒返 42 ⇒ 20/21」；reviewer 用更强同向改坏得 19/21；建议写清「改坏力度 / 被中和的检查项」 | ① 该件尾部补「改坏力度 / 被中和的检查项」说明；② 作者侧把弱/强两种改坏各复现一次（**21/22** vs **20/22**，见 `06`）；③ 该 pack `SHA256SUMS.txt` 重生成 |

## 未收（如实登记）

- reviewer-5 `108fc72b9f01` O-1（@ `9a5d9b11b`）：④ 纯度门 `drop_exp_consumers.tsv` 的 Rust
  `measured`/`setup` 站点行是**文件级前缀**，建议**窗口期**按 `attack_sites.tsv` 的行级锚收紧。
  该件自带「窗口期」口径且属 ④ 真机侧（`drop-exp/**` + 窗口），本片不动、不据以收卡。
- 卡级 **R 真机 DoD**（同场景含怪两相 trace + `groupdiff --fold --strict` 的 site/order diff=0）：
  阻塞 = 承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口 ⇒ **not_verified**，卡保持 in-progress。
