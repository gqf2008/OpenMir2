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
set -e
export MIR2_LEDGER_RUN_ID="${MIR2_LEDGER_RUN_ID:-magic-plane-$(date +%s)-$$}"
echo "RUN_ID=$MIR2_LEDGER_RUN_ID"

# 驱动清单：以 tests::*_parity_* 的 --list 为准（不手抄，防漏支）
DRIVERS=$(cargo test -q -p game-svr --lib -- --ignored --list 2>/dev/null \
  | sed -n 's/^\(tests::[a-z0-9_]*_parity_[a-z0-9_]*\): test$/\1/p')
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

cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored --test-threads=1

# run_id 侧车校验：台账目录里每份 <stem>_<label>.run.json 必须与本次 run_id 一致
[ "${MAGIC_PLANE_SKIP_RUNID_CHECK:-0}" = "1" ] || python -I -B - "$MIR2_LEDGER_RUN_ID" "$PWD/target/magic-parity" <<'PY'
import glob,json,os,sys
want,dirp=sys.argv[1],sys.argv[2]
bad=[f for f in glob.glob(os.path.join(dirp,'*.run.json'))
     if json.load(open(f,encoding='utf-8')).get('run_id')!=want]
print('RUNID_SIDECARS', len(glob.glob(os.path.join(dirp,'*.run.json'))), 'mismatch', len(bad))
sys.exit(1 if bad else 0)
PY
echo "PLANE_OK run_id=$MIR2_LEDGER_RUN_ID"
