$ git diff origin/master(=3db79cae0) -- mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014   # 本收口片的净差量（README/14/SHA256SUMS 三件）
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/14-redcheck-readme-drift.txt b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/14-redcheck-readme-drift.txt
index 773f87413..e5cc39134 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/14-redcheck-readme-drift.txt
+++ b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/14-redcheck-readme-drift.txt
@@ -1,18 +1,24 @@
-# 改坏必红：给一个已登记 README 追改后**不重记**清单 ⇒ verify 必 RED，还原后 GREEN
+# 改坏必红（O-2 收口重抓，2026-10-14）：给一个已登记 README 追改后**不重记**清单 ⇒ verify 必 RED，还原后 GREEN
+
+口径：路径已归一（`<repo>` = 本 worktree 仓库根）；备份件放**仓外**（`E:/tmp/m3cs-o1o2/README.bak`），
+故原抓件里的瞬态残留 `README.md.o1bak`（reviewer-5 O-2）不再出现。`EOL 陷阱:` 段是改坏窗口期的
+清单/HEAD-blob 提示（清单 hash == HEAD blob 而工作树 README 已追加），还原后即消失（见文末 GREEN）。
 
 MUTATE  sha256(before)= 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
+$ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <repo>/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014
 RED   mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\SHA256SUMS.txt  [repo 口径]  OK 15/16
-       mismatch: E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m3cs\mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\README.md
+       mismatch: <repo>\mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\README.md
          清单 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
-         实算 b62920327f0752e149fee78d993a1c1e8a51dffa0414d5c25245a70902ac6cc4  (text attr=set)
-       多件: README.md.o1bak
-
+         实算 40444f19efc3921eda5e6347cf4868ff8c5e112d3438a141fae9d779da5ca574  (text attr=set)
+       EOL 陷阱: <repo>\mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\README.md
+         清单按git blob 原始字节(含 CRLF)记 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
+         仓库口径(LF 归一/git blob)应为 40444f19efc3921eda5e6347cf4868ff8c5e112d3438a141fae9d779da5ca574  (text attr=set)
+         ⇒ 该清单在工作树/提交物自洽、在仓库字节口径/干净检出不可复现（用 `write` 改写）
 rc= 1
 RESTORE sha256(after) = 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783 MATCH
+$ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <repo>/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014
 GREEN mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\SHA256SUMS.txt  [repo 口径]  OK 16/16
 rc= 0
 
-$ git status --porcelain mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014
- M mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/README.md
- M mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/SHA256SUMS.txt
-(空 = README 与清单均已按追改态落盘，无临时残留)
+$ git status --porcelain <repo>/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014   # 还原后应空
+(空 = README 与清单均已按原态落盘，无临时残留)
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/README.md b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/README.md
index 213546517..14b9668a7 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/README.md
+++ b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/README.md
@@ -4,7 +4,7 @@
 - 收口项：reviewer-6 `090d0cc4`（@reviewed_head `f114a5e90`）**O-1** 的残余建议
   ——「这些包在 merge 时的字节与当前 head 不同，建议（**下次动这些文件时**）在**包内 README 或
   whitelist** 追一行『追改原因+日期』，免得日后审计把该漂移误当篡改」。
-- base：`origin/master` = `515c6b5ea`
+- base：`origin/master` = `f43364c45`（交付 base；本包初稿编写时为 `515c6b5ea`，见 §5 O-1 追记）
 - head / branch：`worktree-m3-chain-gate-o1-retro`
 - 作者：`openmir2-worker-5`；**待独立 principal 复核**
 - tier：**L**（纯文档/证据片：零 `.rs`、零 `.toml`、零 `.ps1`、零 `.cs`，零运行时；
@@ -42,7 +42,7 @@
 | 链门 check | `FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0` rc=0（含 `fixture_fmt_evidence_check GREEN（9 个证据包 / 9 件）`） | `05` |
 | fmt 证据门 selftest/check/mutation | `SELFTEST GREEN 8/8` · `FIXTURE_FMT_EVIDENCE GREEN（9/9）` · `MUTATION GREEN 5/5` 均 rc=0 | `06`–`08` |
 
-> 链门 check 在**当前 master `515c6b5ea` 的树**上 GREEN（本片 base）；上一 tick（`9edfc745d`）那条
+> 链门 check 在**当前 master `f43364c45` 的树**上 GREEN（本片交付 base；初稿编写时为 `515c6b5ea`，见 §5）；上一 tick（`9edfc745d`）那条
 > `FIXTURE_CHAIN RED`（RNG 行号锚漂移）已由 m3bag 的根因片
 > `wip/m3bag-anchor-teeth`（撤行号锚改「符号存在且唯一」机判）合入 master 后自动消失 —— 本片不涉。
 
@@ -55,7 +55,7 @@
 | `cargo test --workspace` | 101 suites / 1219 passed / 0 failed / 22 ignored，rc=0 | `11` |
 | `cargo run -p msgcodegen -- --check` | 604 条一致，rc=0 | `12` |
 
-> 本片零 `.rs`/`.toml`/`.cs` ⇒ 四门禁读数预期与 base `515c6b5ea` 一致，此处为**本片树上实测**。
+> 本片零 `.rs`/`.toml`/`.cs` ⇒ 四门禁读数预期与 base `f43364c45` 一致，此处为**本片树上实测**。
 
 ## 4. 边界与残留
 
@@ -69,3 +69,19 @@
 - 卡级真机 DoD（各 READY 夹具 `run_*.ps1` 两侧同序列 `diff=0`）仍等世界侧实现 +
   `openmir2-m3-clock-injection` + owner 独占窗口，本片不据此收卡。
 - 起栈 0 / 端口 0；未自签 review；未 merge；只在 `m3cs` worktree 写/提交。
+
+## 5. O-1 / O-2 收口（2026-10-14，后续小片；本包主体已合入 master `ffbaaa72e`）
+
+reviewer-5（approve @reviewed_head `f58b8dfa6`）在本包主体上留了两条非阻塞 Minor，随后由**独立后续小片**按点收口：
+
+| # | 原文 | 处置 |
+| --- | --- | --- |
+| **O-1**（Minor·文档时效） | 本包 README 写 base / 链门树为 `515c6b5ea`，而交付 patch 的 base 是 `f43364c45`（此后两次 `merge origin/master`） | 订正：头部 base、§2 链门树说明、§3 四门禁说明三处 `515c6b5ea` → `f43364c45`，并各留一句「初稿编写时为 `515c6b5ea`」以免后人把这次订正当篡改 |
+| **O-2**（Minor·证据卫生） | `14-redcheck-readme-drift.txt` 原始输出含作者 worktree 绝对路径，且当时留下临时件 `README.md.o1bak`（RED 行含「多件: README.md.o1bak」） | 重抓：① 备份件移到**仓外**（`E:/tmp/m3cs-o1o2/README.bak`）⇒ 不再有 `.o1bak` 瞬态、也不再出现「多件」行；② 输出里的仓库根绝对路径归一为 `<repo>`；③ 牙齿结论不变（追改不重记 ⇒ RED 15/16 点名 `README.md`；按 sha256 还原 ⇒ GREEN 16/16）。原 `14` 的 sha256 = `7695d46aa3f17a25d811f65e7f3f2d972f69fdcaf0c7ce89a73ddd8692484c4a`（重抓前的历史值，留档） |
+
+> O-2 重抓件里出现的 `EOL 陷阱:` 段是 `verify_sha256sums.py` 的**改坏窗口期**提示（清单 hash 恰好等于 HEAD blob，而工作树 `README.md` 已被追加 ⇒ 工具把「清单 == HEAD blob」当可能的行尾陷阱列出）——**不是 EOL 缺陷**：还原后 `GREEN 16/16`，本包 `eolcheck 18/18` GREEN（`15-self-verify.txt`）。原版 `14` 未触发该提示，是因为当时该 README 尚未提交（HEAD 无该 blob）。
+>
+> 本收口片仍为 L 级（零 `.rs`/`.toml`/`.ps1`/`.cs`，只动本包 `README.md` 与 `14-*` 两件 + 重记清单）；不引入任何 gating 逻辑改动。
+>
+> **`15-self-verify.txt` 保持修前读数**（`OK 17/17` 仍然成立，只是不含本次的新 hash）；本收口片的**修后**逐件读数在
+> 新证据包 `../evidence-m3-chain-gate-o1o2-closeout-20261014/`（`verify --check-extra` / `eolcheck` / 链门 / fmt 证据门 / 四门禁 / 边界）。
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/SHA256SUMS.txt b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/SHA256SUMS.txt
index 11d76a6d9..3dee96c5d 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/SHA256SUMS.txt
+++ b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014/SHA256SUMS.txt
@@ -12,6 +12,6 @@ ebce974175a05b23d50b7f9eb7e9db80f393df7e41da4ab0003da0f3f2f3feb9  10-gate-clippy
 ef53aa523a90dea6d4246522a9d466f591deb982fb9d042fefb70fe41aae14e3  11-gate-test.txt
 ce60a156e795fda4dc1f355dec08871c988b21cc291ac6488246ecaacb5fa223  12-gate-msgcodegen.txt
 09121b38225c00cc8959b29d10fdf53cecf0b86f09058c055537bff96573c976  13-boundary.txt
-7695d46aa3f17a25d811f65e7f3f2d972f69fdcaf0c7ce89a73ddd8692484c4a  14-redcheck-readme-drift.txt
+5689ca6f289129fb7dd9c0f128b22e367ac4a37fb8d1abd6b8715b40e610f1d5  14-redcheck-readme-drift.txt
 8bb71adb0451061c78949cff2f75d3308db4c23411ffc89bf192106c4cf77af1  15-self-verify.txt
-153d3666e28944d3a33bd4d00077d16f6b16d3096cfa2505c6912a1b9f404dd5  README.md
+5a1ffbad524622ed669df27c0f1e25701f41eb35c0b39a7ca4b1babb74a3540d  README.md

$ git diff --stat origin/master -- mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o1-retro-20261014
 .../14-redcheck-readme-drift.txt                   | 24 ++++++++++++++--------
 .../README.md                                      | 22 +++++++++++++++++---
 .../SHA256SUMS.txt                                 |  4 ++--
 3 files changed, 36 insertions(+), 14 deletions(-)
