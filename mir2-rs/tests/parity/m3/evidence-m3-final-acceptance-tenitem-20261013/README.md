# evidence-m3-final-acceptance-tenitem-20261013

卡 `openmir2-m3-final-acceptance` 的「DoD① 十项表补实 + whitelist 新登记」片证据
（owner comment `256cec70774f` 的 todo；仅 `mir2-rs/tests/parity/**` 文档改动）。

| 文件 | 内容 |
| --- | --- |
| `00-boundary.txt` | 写集边界：`git diff --stat origin/master` + `-- src sql`（空） |
| `01-gates.txt` | 四门禁读数（fmt/clippy/test/msgcodegen） |

口径：`SHA256SUMS.txt` 按仓库字节（Git blob / LF 归一）记；复核：

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-final-acceptance-tenitem-20261013
```
