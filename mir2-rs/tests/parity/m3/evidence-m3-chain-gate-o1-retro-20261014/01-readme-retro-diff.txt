$ git diff --stat -- <5 pkg>/README.md <5 pkg>/SHA256SUMS.txt

 .../parity/m3/evidence-m3-anchor-prefix-o2-20261014/README.md    | 9 +++++++++
 .../m3/evidence-m3-anchor-prefix-o2-20261014/SHA256SUMS.txt      | 2 +-
 .../tests/parity/m3/evidence-m3-chain-gate-o2-20261014/README.md | 9 +++++++++
 .../parity/m3/evidence-m3-chain-gate-o2-20261014/SHA256SUMS.txt  | 2 +-
 .../m3/evidence-m3-chain-gate-residuals-20261014/README.md       | 9 +++++++++
 .../m3/evidence-m3-chain-gate-residuals-20261014/SHA256SUMS.txt  | 2 +-
 .../m3/evidence-m3-fixture-chain-hero-anchor-20261014/README.md  | 9 +++++++++
 .../SHA256SUMS.txt                                               | 2 +-
 .../m3/evidence-m3-fixture-family-minors-20261014/README.md      | 9 +++++++++
 .../m3/evidence-m3-fixture-family-minors-20261014/SHA256SUMS.txt | 2 +-
 10 files changed, 50 insertions(+), 5 deletions(-)

$ git diff -- <5 pkg>/README.md

diff --git a/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/README.md b/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/README.md
index 1915206cf..a8bd5eed2 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/README.md
+++ b/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/README.md
@@ -77,3 +77,12 @@ python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity
   `openmir2-m3-clock-injection` + owner 独占 `-Go` 窗口；本片不据此收卡。
 - reviewer-5 `2ce5d87b` 的 **O-A**（真实目录名恰形如 `3966-4105` 且紧跟 `:` 会被当 spec 区间）
   与 **O-C**（路径计数文档口径）非本片范围；O-C 已在 O-1 片 `610d7258` 订正。
+
+## 追改登记（retro-fix record，2026-10-14）
+
+> 卡 `openmir2-m3-fixture-chain-gate`；reviewer-6 `090d0cc4`（@`f114a5e90`）**O-1** 的收口片 `becf075db`。
+> 本包 `40-fmt.txt` 原为 **0 B（空）**，追改为自描述体例（`$ cargo fmt --all -- --check` + `rc=0`）；
+> `SHA256SUMS.txt` 同步按**仓库口径（LF 归一 = git blob 字节）**重记。
+> **方向为增强、非篡改**（原空件不能自证 fmt 绿）。5 包清单与前后对照见
+> `../evidence-m3-fixture-fmt-evidence-20261014/README.md` §1。
+> 本次 README 追记（O-1 的「包内登记」）同样在该约定下重记本包清单。
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/README.md b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/README.md
index 7ba6ca1ab..9c744fd34 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/README.md
+++ b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/README.md
@@ -88,3 +88,12 @@ def _drop_one_guard(guard_markers):
    需 `mir2-rs/tests/parity/world/**` 写集（非本车道）⇒ 本片只接 `selftest` 向，未接 `--strict`。
 2. 卡级真机 DoD（⑧-6/⑧-10/⑨-5 各 READY 夹具 `run_*.ps1` 两侧同序列 diff=0）仍需
    `openmir2-m3-clock-injection` + owner 独占窗口；本片不涉、不据此收卡。
+
+## 追改登记（retro-fix record，2026-10-14）
+
+> 卡 `openmir2-m3-fixture-chain-gate`；reviewer-6 `090d0cc4`（@`f114a5e90`）**O-1** 的收口片 `becf075db`。
+> 本包 `20-gate-fmt.txt` 原为 **0 B（空）**，追改为自描述体例（`$ cargo fmt --all -- --check` + `rc=0`）；
+> `SHA256SUMS.txt` 同步按**仓库口径（LF 归一 = git blob 字节）**重记。
+> **方向为增强、非篡改**（原空件不能自证 fmt 绿）。5 包清单与前后对照见
+> `../evidence-m3-fixture-fmt-evidence-20261014/README.md` §1。
+> 本次 README 追记（O-1 的「包内登记」）同样在该约定下重记本包清单。
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/README.md b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/README.md
index a39c6552a..fa365ab61 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/README.md
+++ b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/README.md
@@ -91,3 +91,12 @@ check("guards-missing-real-guard-red",
   `duplicates=[42]`（T-42 撞号）是已路由 owner 的历史问题，本片**不碰**（同上一片口径）。
 - `check_window_batch.py` / `compare_dedicated_streams.py` 等为窗口期辅助件，不在「在场却坏」
   的 fail-closed 门之列，未登记。
+
+## 追改登记（retro-fix record，2026-10-14）
+
+> 卡 `openmir2-m3-fixture-chain-gate`；reviewer-6 `090d0cc4`（@`f114a5e90`）**O-1** 的收口片 `becf075db`。
+> 本包 `10-gate-fmt.txt` 原为 **5 B（仅 `rc=0`，无命令头）**，追改为自描述体例
+> （`$ cargo fmt --all -- --check` + `rc=0`）；`SHA256SUMS.txt` 同步按**仓库口径（LF 归一 = git blob 字节）**重记。
+> **方向为增强、非篡改**（原件不能自证 fmt 绿）。5 包清单与前后对照见
+> `../evidence-m3-fixture-fmt-evidence-20261014/README.md` §1。
+> 本次 README 追记（O-1 的「包内登记」）同样在该约定下重记本包清单。
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/README.md b/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/README.md
index 011c037a9..d251a0ff1 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/README.md
+++ b/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/README.md
@@ -87,3 +87,12 @@ RED）。三条锚门现在同等受保护。这与 ⑧-6 O-1 对 `castle_oracle
   `04b.txt`（改坏必红 B）、`bnd.txt`（`09` 来源）；另有 `E:/tmp/hps.bak`（改坏必红 A 前的
   `hero_pet_scenario.json` 备份，已还原）、`E:/tmp/fcg.bak`（改坏必红 B 前的
   `fixture_chain_gate.py` 备份，已还原）。
+
+## 追改登记（retro-fix record，2026-10-14）
+
+> 卡 `openmir2-m3-fixture-chain-gate`；reviewer-6 `090d0cc4`（@`f114a5e90`）**O-1** 的收口片 `becf075db`。
+> 本包 `05-gate-fmt.txt` 原为 **1 B（仅换行）**，追改为自描述体例（`$ cargo fmt --all -- --check` + `rc=0`）；
+> `SHA256SUMS.txt` 同步按**仓库口径（LF 归一 = git blob 字节）**重记。
+> **方向为增强、非篡改**（原件不能自证 fmt 绿）。5 包清单与前后对照见
+> `../evidence-m3-fixture-fmt-evidence-20261014/README.md` §1。
+> 本次 README 追记（O-1 的「包内登记」）同样在该约定下重记本包清单。
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/README.md b/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/README.md
index d61f7ad55..3c778b5fc 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/README.md
+++ b/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/README.md
@@ -67,3 +67,12 @@ fail-closed 表 ⇒ 在 **master 上**该门恒 RED ⇒ 常跑的 `fixture_chain
    owner 独占窗口）**不在本片范围**，卡保持 `in-progress`。
 3. `evidence-m3-hero-pet-anchor-20261014/08-gate-test.txt` 为分支上原始转录（无汇总行），
    README/`00-commands.txt` 现标注为**合并树读数**（1200）。
+
+## 追改登记（retro-fix record，2026-10-14）
+
+> 卡 `openmir2-m3-fixture-chain-gate`；reviewer-6 `090d0cc4`（@`f114a5e90`）**O-1** 的收口片 `becf075db`。
+> 本包 `08-gate-fmt.txt` 原为 **0 B（空）**，追改为自描述体例（`$ cargo fmt --all -- --check` + `rc=0`）；
+> `SHA256SUMS.txt` 同步按**仓库口径（LF 归一 = git blob 字节）**重记。
+> **方向为增强、非篡改**（原空件不能自证 fmt 绿）。5 包清单与前后对照见
+> `../evidence-m3-fixture-fmt-evidence-20261014/README.md` §1。
+> 本次 README 追记（O-1 的「包内登记」）同样在该约定下重记本包清单。

$ git diff -- <5 pkg>/SHA256SUMS.txt

diff --git a/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/SHA256SUMS.txt b/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/SHA256SUMS.txt
index c90802548..cabe14741 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/SHA256SUMS.txt
+++ b/mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014/SHA256SUMS.txt
@@ -24,4 +24,4 @@ b508e3d0749648b80e99356f6978946b015e06e9bbbed85fbf4f4dc88e7e8792  32-chain-gate-
 aa954753dffe051224079c90bdea2b9a25ca717188e9d5b609cb9f3a6175aee1  41-clippy.txt
 fd7b2a2de3981a5fdea95bd64e3749ea593f9d27033366af91797dea4e7fc032  43-test-summary.txt
 99613aace745de5095ff1ae9eab9ff37bfbcaf42798d69d2a883dba791b33d23  44-msgcodegen.txt
-136f4b984317dbf4bbc18e814b923f5797a6a1d8d2f69ea5bb6b4fb061fe5c0a  README.md
+f1ca74faeecab9d9673504c36269748c15d2d07f6f94ef2fcee6e2d110e8c123  README.md
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/SHA256SUMS.txt b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/SHA256SUMS.txt
index b17982b9f..2fcaecf7a 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/SHA256SUMS.txt
+++ b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014/SHA256SUMS.txt
@@ -15,4 +15,4 @@ b577d92fec50e8c5e8f7050c25109efe475e0a2512c4086ccc64b360a5f47397 *21-gate-clippy
 b4fee9ffee3aa0ad9a4b81aef69b7278a3b9897a26bc439a30d4fe8bb40d5e98 *23-gate-test.txt
 208fcb98af269228255b331862901c100a88d75d58d40cf693f7b8de0bba2682 *30-boundary.txt
 b2349aad09c98d460dc1ec5a2adf21f9e621475a4f259b69137129b50ea4e98a *31-code-diff.txt
-5d957e8811b87132b4fe9609b424e24229c8f0553bf29a76bf1450d2a370e278 *README.md
+ddc3cc25a07242e33872fd0f980abd2dc1a9ee0563874e5436edd5a283440242 *README.md
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/SHA256SUMS.txt b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/SHA256SUMS.txt
index 214220be6..d16c852dd 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/SHA256SUMS.txt
+++ b/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/SHA256SUMS.txt
@@ -13,4 +13,4 @@ be9fb94447659b6c0cc454fffb7879094ffeb7238c26f421028ba046a49f23e6 *11-gate-clippy
 e00f5c40336a883ba55dc69fc8958bb44a1c42fd284aa8d39415c1686418eb77 *12-gate-test.txt
 b0eafd92f1bfe90aae01891b87ba4b2b1d046b456fc795deb6263b8f953ba411 *13-gate-msgcodegen.txt
 9fcf921e656710c19df403ad0fc85ac8ac060a020cb5c100298caf8914bf53ba *14-boundary.txt
-7c47cb307612cef3231374644ad706c026d1284d3bc0803881746d3de0272ebf *README.md
+77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783 *README.md
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/SHA256SUMS.txt b/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/SHA256SUMS.txt
index ce84cb26a..d0e72eef7 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/SHA256SUMS.txt
+++ b/mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014/SHA256SUMS.txt
@@ -9,4 +9,4 @@ aae5470fc8877dabcb9d628970a7590be2f613bd6c917bc9d7cef6ebc80b0e2a  04-redcheck.tx
 974cdfb633fae64f4a695759a466437e17c6a79108e22bb83483954e282d115d  08-gate-msgcodegen.txt
 e31495d9e56c300355ae0a989f3d913f838726012781e219d6daffcf6f3b41e8  09-boundary.txt
 a1856ba86cdb4b8d708b3b082cfa1258b611c7de5e12efdb948893fbb23a364e  10-hash-check.txt
-3c728389c3c95e92f50591ea76ed2620ccc1a6361a1f63a1f4cda97edfc2f80d  README.md
+b591140d65c1fd4c0a6bb80bebcd8e6acfe4c3860e4227d5bdbea9434d689437  README.md
diff --git a/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/SHA256SUMS.txt b/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/SHA256SUMS.txt
index 7162f68e4..ba0189381 100644
--- a/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/SHA256SUMS.txt
+++ b/mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014/SHA256SUMS.txt
@@ -11,4 +11,4 @@ dbf409295d31728dec8987a1df083ef9c2dfe7b3c8d83481ebb85141e6e57953 *03-hero-pet-an
 fde90ce56091f1677871d900c20e07eec8c7ce9a8278a6adbf128d0d94263e52 *10-gate-test.txt
 974cdfb633fae64f4a695759a466437e17c6a79108e22bb83483954e282d115d *11-gate-msgcodegen.txt
 e661e533b2e604065f2ada6536b03b4cf93f5532c9b606802ddacb2efff9318a *12-boundary.txt
-f0b4d93b2f6299eaf2401a62632eb764a6c3977bb40abbe8737f47c154c3067b *README.md
+cd6e73d9da4b6c1b7d277c0ad5be583d799d5c974b8bfb871768a575368e9019 *README.md
