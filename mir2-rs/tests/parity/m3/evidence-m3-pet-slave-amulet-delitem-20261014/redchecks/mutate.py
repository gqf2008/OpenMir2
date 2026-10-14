#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""⑨-2 护身符扣光帧面 的改坏必红变异脚本（卡 openmir2-m3-pet-slave-amulet-delitem-frame）。

用法（cwd = 任意；路径相对本 worktree 根）：

    python -I mutate.py m1        # 变异 1：deleted 支不再发帧（回到登记前）
    python -I mutate.py m2        # 变异 2：帧内 Dura 用扣光前的值（不做 Dura=0）
    python -I mutate.py m3        # 变异 3：去掉 StdMode==50 的名称后缀
    python -I mutate.py restore   # 从 .bak 还原全部被变异文件

口径：首次变异前把原文写成 `<file>.mutbak`；`restore` 从 `.mutbak` 还原并删除。
还原后用 `git hash-object <file>` 与 `git rev-parse HEAD:<file>` 对拍（必须相等）。
"""
import io
import os
import sys

# 本文件 = <root>/mir2-rs/tests/parity/m3/evidence-xxx/redchecks/mutate.py
#   → 上溯 6 层到 worktree 根
_HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(_HERE, *([".."] * 6)))
assert os.path.isdir(os.path.join(ROOT, "mir2-rs", "bins", "game-svr")), ROOT
MAIN = os.path.join(ROOT, "mir2-rs", "bins", "game-svr", "src", "main.rs")
SLAVE = os.path.join(ROOT, "mir2-rs", "bins", "game-svr", "src", "slave.rs")

# (文件, 原文, 变异)
MUTATIONS = {
    "m1": (
        MAIN,
        "                        apply_amulet_use(ground_sessions.get_mut(chr_name), slot, used);\n"
        "                        if let Some(body) = body {\n"
        "                            send_self_frame(online, gates, chr_name, body);\n"
        "                        }\n",
        "                        apply_amulet_use(ground_sessions.get_mut(chr_name), slot, used);\n"
        "                        let _ = body;\n",
    ),
    "m2": (
        SLAVE,
        "        make_index: item.make_index,\n"
        "        dura: 0,\n"
        "        dura_max: item.dura_max,\n",
        "        make_index: item.make_index,\n"
        "        dura: item.dura,\n"
        "        dura_max: item.dura_max,\n",
    ),
    "m3": (
        SLAVE,
        '    if tpl.std_mode == 50 {\n'
        '        // oracle `clientItem.Item.Name = clientItem.Item.Name + " #" + userItem.Dura;`'
        "（Dura 已置 0）。\n"
        '        name = format!("{name} #0");\n'
        "    }\n",
        "    let _ = tpl.std_mode;\n",
    ),
}


def _read(path):
    return io.open(path, encoding="utf-8", newline="").read()


def _write(path, text):
    io.open(path, "w", encoding="utf-8", newline="").write(text)


def _backup(path):
    bak = path + ".mutbak"
    if not os.path.exists(bak):
        _write(bak, _read(path))


def apply(name):
    path, old, new = MUTATIONS[name]
    _backup(path)
    text = _read(path)
    assert text.count(old) == 1, (path, name, text.count(old))
    _write(path, text.replace(old, new))
    print(f"mutated {name}: {path}")


def restore():
    for path in {MAIN, SLAVE}:
        bak = path + ".mutbak"
        if os.path.exists(bak):
            _write(path, _read(bak))
            os.remove(bak)
            print(f"restored {path}")
    # main.rs 在 m1 中被变异；若只变异过 slave.rs，main.rs 无 .mutbak ⇒ 不动。


if __name__ == "__main__":
    arg = sys.argv[1] if len(sys.argv) > 1 else ""
    if arg == "restore":
        restore()
    elif arg in MUTATIONS:
        apply(arg)
    else:
        raise SystemExit("用法: mutate.py m1|m2|m3|restore")
