# 改坏必红：给一个已登记 README 追改后**不重记**清单 ⇒ verify 必 RED，还原后 GREEN

MUTATE  sha256(before)= 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
RED   mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\SHA256SUMS.txt  [repo 口径]  OK 15/16
       mismatch: E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m3cs\mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\README.md
         清单 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
         实算 b62920327f0752e149fee78d993a1c1e8a51dffa0414d5c25245a70902ac6cc4  (text attr=set)
       多件: README.md.o1bak

rc= 1
RESTORE sha256(after) = 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783 MATCH
GREEN mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\SHA256SUMS.txt  [repo 口径]  OK 16/16
rc= 0

$ git status --porcelain mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014
 M mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/README.md
 M mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014/SHA256SUMS.txt
(空 = README 与清单均已按追改态落盘，无临时残留)
