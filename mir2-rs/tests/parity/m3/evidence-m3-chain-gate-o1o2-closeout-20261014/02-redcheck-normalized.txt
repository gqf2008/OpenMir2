# 改坏必红（O-2 收口重抓，2026-10-14）：给一个已登记 README 追改后**不重记**清单 ⇒ verify 必 RED，还原后 GREEN

口径：路径已归一（`<repo>` = 本 worktree 仓库根）；备份件放**仓外**（`E:/tmp/m3cs-o1o2/README.bak`），
故原抓件里的瞬态残留 `README.md.o1bak`（reviewer-5 O-2）不再出现。`EOL 陷阱:` 段是改坏窗口期的
清单/HEAD-blob 提示（清单 hash == HEAD blob 而工作树 README 已追加），还原后即消失（见文末 GREEN）。

MUTATE  sha256(before)= 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
$ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <repo>/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014
RED   mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\SHA256SUMS.txt  [repo 口径]  OK 15/16
       mismatch: <repo>\mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\README.md
         清单 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
         实算 40444f19efc3921eda5e6347cf4868ff8c5e112d3438a141fae9d779da5ca574  (text attr=set)
       EOL 陷阱: <repo>\mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\README.md
         清单按git blob 原始字节(含 CRLF)记 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783
         仓库口径(LF 归一/git blob)应为 40444f19efc3921eda5e6347cf4868ff8c5e112d3438a141fae9d779da5ca574  (text attr=set)
         ⇒ 该清单在工作树/提交物自洽、在仓库字节口径/干净检出不可复现（用 `write` 改写）
rc= 1
RESTORE sha256(after) = 77fcfec5faa795c21f1eb59109d14526ea6c71382bf2a8cd047a7be7c7d00783 MATCH
$ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <repo>/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014
GREEN mir2-rs\tests\parity\m3\evidence-m3-chain-gate-residuals-20261014\SHA256SUMS.txt  [repo 口径]  OK 16/16
rc= 0

$ git status --porcelain <repo>/mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014   # 还原后应空
(空 = README 与清单均已按原态落盘，无临时残留)
