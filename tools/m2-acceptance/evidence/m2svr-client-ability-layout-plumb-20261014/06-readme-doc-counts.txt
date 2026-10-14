# 06 — 文档漂移门：README §0b.4 读数与可执行判据/证据绑定（reviewer-5/6 同一 Minor 收口）
#
# 背景：README §0b.4 曾写 `15/15` + 红检 `②a/③a`，实际为 `20/20` + `②a/②c/③a/⑤b`
#   （reviewer-5 e74f870a / reviewer-6 ad719ce7 对片㊷ 的同一非阻塞 Minor）。
# 收口：① 订正 README 该行；② 订正判据脚本 ⑤b Detail 文案；③ 新增静态门
#   tools/m2-acceptance/check_client_ability_layout_doc_counts.py（纯文件读取，零进程/零端口）。
#
# 门项：D1 `N/M` 自洽；D2 README N == 脚本 `Check '` 条数（活口径）；
#       D3 README N == 证据 02 `SUMMARY total`；D4 README 红检 id 集 == 证据 03 `[FAIL]` 集。

# ---- GREEN：python -I -B tools/m2-acceptance/check_client_ability_layout_doc_counts.py ----
  [OK  ] D0 README 含 `<N>/<M> ⇒ CLIENT_ABILITY_LAYOUT_PLUMB GREEN` 行
        doc_line=# 20/20 ⇒ CLIENT_ABILITY_LAYOUT_PLUMB GREEN (rc=0)；删掉注入行 ⇒ ②a/②c/③a/⑤b 必红 (rc=1)
  [OK  ] D1 `N/M` 形式自洽（N == M）
        N=20 M=20
  [OK  ] D2 README N == 脚本 Check 条数（活口径 = 20）
        N=20 script_check_count=20
  [OK  ] D3 README N == 证据 02 SUMMARY total（= 20）
        N=20 evidence_total=20
  [OK  ] D4 README 改坏必红 id 集 == 证据 03 [FAIL] 集（顺序无关）
        doc=['②a', '②c', '③a', '⑤b'] evidence=['②a', '②c', '③a', '⑤b']
SUMMARY total=5 ok=5 fail=0
CLIENT_ABILITY_LAYOUT_DOC_COUNTS GREEN

# ---- 改坏必红：把 README §0b.4 改回 `15/15` + `②a/③a`（sha256(README before)=2b316a12…）----
# 命令：python -I -B tools/m2-acceptance/check_client_ability_layout_doc_counts.py
  [OK  ] D0 README 含 `<N>/<M> ⇒ CLIENT_ABILITY_LAYOUT_PLUMB GREEN` 行
        doc_line=# 15/15 ⇒ CLIENT_ABILITY_LAYOUT_PLUMB GREEN (rc=0)；删掉注入行 ⇒ ②a/③a 必红 (rc=1)
  [OK  ] D1 `N/M` 形式自洽（N == M）
        N=15 M=15
  [FAIL] D2 README N == 脚本 Check 条数（活口径 = 20）
        N=15 script_check_count=20
  [FAIL] D3 README N == 证据 02 SUMMARY total（= 20）
        N=15 evidence_total=20
  [FAIL] D4 README 改坏必红 id 集 == 证据 03 [FAIL] 集（顺序无关）
        doc=['②a', '③a'] evidence=['②a', '②c', '③a', '⑤b']
SUMMARY total=5 ok=2 fail=3
CLIENT_ABILITY_LAYOUT_DOC_COUNTS FAIL
# rc(mutated) = 1
# 改动已还原（README sha256 回 2b316a122b349102360e3a70f032aae357eb50d67f140c012b67301e86a08c65），复跑 GREEN。
