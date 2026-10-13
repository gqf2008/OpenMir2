# README 计数订正（F-1）：BEFORE(origin/master) §6 仍写 `16/16`（上一片 gate selftest 16→21 漏改）；
# AFTER(HEAD) §5/§6 = `22/22`（与实测一致；本片再 +1 stream-pair-prereq-red）。

=== BEFORE (base) —— 含 16/16 / 21/21 的行 ===
（16/16）python -I rng-trace-scene/rng_scene_gate.py selftest           # 16/16 GREEN（含 scenario 声明 vs 代码）
（21/21）自检覆盖（27 项；全离线）：scenario 固定输入、MonGen 冻结件零行、场景门 selftest（21/21）、

=== AFTER (HEAD) —— 含 16/16 / 21/21 / 22/22 的行 ===
（16/16）(无)
（21/21）(无)
（22/22）自检覆盖（27 项；全离线）：scenario 固定输入、MonGen 冻结件零行、场景门 selftest（22/22）、
python -I rng-trace-scene/rng_scene_gate.py selftest           # 22/22 GREEN（含 scenario 声明 vs 代码 + 半给流水 PREREQ）
