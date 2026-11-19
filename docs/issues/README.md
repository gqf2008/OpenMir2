# docs/issues 机器清单

`parity-gap-items.json` / `parity-gap-quests.json` / `parity-gap-npc.json` / `parity-gap-magic.json`
是覆盖率的**唯一机器事实源**（schema 见 `openmir2-parity-gap-items-quests-npc.md` §12）。

- 改任何条目的 `status` 时，必须同时改该条 `notes` 说明依据，并运行 `coverage-report.ps1` 自检；
- 覆盖率脚本切到 master 之前，不要移动这些文件的位置；
- 人类可读汇总与口径解释在 `openmir2-parity-gap-items-quests-npc.md`；两者不一致时以 JSON 为准并回改文档。
