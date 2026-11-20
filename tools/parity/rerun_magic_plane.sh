#!/bin/sh
# rerun_magic_plane.sh — magic 对拍全平面复跑（驱动 → 台账 → 比较器），一条命令走完两步链。
#
# 为什么需要：magic 的 20 个 real_rust_ledger_* 全部 `#[ignore]`；默认 `cargo test` 一条都不跑。
# 真实复跑链 = ① 逐支驱动（rng_trace 进程级，必须 --exact --ignored 单跑）② 台账比较器（--ignored，同 run_id）。
#
# 用法（在 mir2-rs/ 下）：sh ../tools/parity/rerun_magic_plane.sh
#   MIR2_LEDGER_RUN_ID  可预置；缺省自动生成并以 RUN_ID= 行回显
#   CARGO_BUILD_JOBS    透传（车道约定：起构建前自行确认内存/车道）
#   MAGIC_PLANE_SKIP_RUNID_CHECK=1  跳过最后的 run_id 侧车校验（不建议）
#   MAGIC_PLANE_ONLY=<stem>         只跑该件（驱动 tests::<stem>_parity_* + 比较器 real_rust_ledger_<stem>）
set -e
export MIR2_LEDGER_RUN_ID="${MIR2_LEDGER_RUN_ID:-magic-plane-$(date +%s)-$$}"
echo "RUN_ID=$MIR2_LEDGER_RUN_ID"

# 驱动清单：以 tests::*_parity_* 的 --list 为准（不手抄，防漏支）
LIST_RAW=$(cargo test -q -p game-svr --lib -- --ignored --list 2>/dev/null)
echo "--- --list 原始输出（驱动自动发现的审计面）---"
echo "$LIST_RAW" | grep '_parity_' || true
echo "--- end --list ---"
DRIVERS=$(echo "$LIST_RAW" | awk '/_parity_/ && /: test$/ {print $1}')
if [ -n "${MAGIC_PLANE_ONLY:-}" ]; then
  DRIVERS=$(echo "$DRIVERS" | grep "^tests::${MAGIC_PLANE_ONLY}_parity_" || true)
  [ -n "$DRIVERS" ] || { echo "ONLY_NO_DRIVER：${MAGIC_PLANE_ONLY} 无匹配驱动"; exit 2; }
fi
[ -n "$DRIVERS" ] || { echo "DRIVERS_EMPTY：--list 未取到任何 *_parity_* 驱动（crate 名/过滤是否变了？）"; exit 2; }
echo "DRIVERS=$(echo "$DRIVERS" | wc -l)"

fail=0
for t in $DRIVERS; do
  if cargo test -q -p game-svr --lib -- --exact --ignored "$t" >/dev/null 2>&1; then
    echo "  ok  $t"
  else
    echo "  FAIL $t"; fail=1
  fi
done
[ $fail -eq 0 ] || { echo "DRIVER_FAIL"; exit 1; }

if [ -n "${MAGIC_PLANE_ONLY:-}" ]; then
  cargo test -p mir2-parity-tests --test magic_fireball_parity -- --exact --ignored "real_rust_ledger_${MAGIC_PLANE_ONLY}" --test-threads=1
else
  cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored --test-threads=1
fi

# run_id 侧车校验：台账目录里每份 <stem>_<label>.run.json 必须与本次 run_id 一致
[ "${MAGIC_PLANE_SKIP_RUNID_CHECK:-0}" = "1" ] || python -I -B - "$MIR2_LEDGER_RUN_ID" "$PWD/target/magic-parity" <<'PY'
import glob,json,os,sys
want,dirp=sys.argv[1],sys.argv[2]
only=os.environ.get('MAGIC_PLANE_ONLY','')
pat=(only+'_*.run.json') if only else '*.run.json'
bad=[f for f in glob.glob(os.path.join(dirp,pat))
     if json.load(open(f,encoding='utf-8')).get('run_id')!=want]
print('RUNID_SIDECARS', len(glob.glob(os.path.join(dirp,pat))), 'mismatch', len(bad))
sys.exit(1 if bad else 0)
PY
echo "PLANE_OK run_id=$MIR2_LEDGER_RUN_ID"
