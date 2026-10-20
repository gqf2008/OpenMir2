#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""prepare_safe_login.py — 窗口受控登录夹具：把 `mir2_test_*` 库里的指定角色摆到安全格。

为什么需要它：真机窗口做「同账号两侧（C# oracle / Rust）对比」时，两侧必须从**同一份角色
行状态**登录，否则 52 SM_ABILITY 的 HP/MaxHP、50 SM_LOGON 的朝向等会因为上一侧登出写回而漂移。
本工具只做一件事：把 `characters` + `characters_ablity` 的少量列摆成给定值，并把**改动前后的
关键列行快照**打到 stdout（证据四件套里的「输出」）。

硬约束（工具自身强制执行，不靠使用人自觉）：
  * **库名守卫**：`--db` 必须以 `mir2_test_` 开头且后面还有字符；`mir2_db` / `mir2_account`
    / `mir2_data` / 空后缀一律拒绝（rc=2，且不执行任何 SQL）。
  * **默认 dry-run**：不给 `--yes-apply` 时只读 + 打印计划 SQL，绝不写库。
  * **单事务**：应用路径是 `START TRANSACTION; UPDATE characters; UPDATE characters_ablity; COMMIT;`，
    mysql 客户端遇错即停（未到 COMMIT 即随连接关闭回滚）。
  * **改后校验**：应用后再查一次，逐列核对目标值；对不上 ⇒ rc=1（RED），不静默成功。

写库方式：不依赖 Python MySQL 驱动，直接调 `mysql.exe --batch`（与 `tools/dbsnap/dbsnap.py`
同口径），因此本脚本可在只装了 MariaDB 客户端的窗口机上跑。

用法：
  # 1) 先看：打印当前行 + 计划 SQL（不写库）
  python -I -B tools/loginfixture/prepare_safe_login.py \
      --db mir2_test_db --char winplay0 --level 1 --hp 19 --max-hp 19

  # 2) 确认后应用（默认安全格 = map "0", (289,618)）
  python -I -B tools/loginfixture/prepare_safe_login.py \
      --db mir2_test_db --char winplay0 --level 1 --hp 19 --max-hp 19 --yes-apply

  # 3) 自检（合成数据 + 假 mysql runner；不连库）
  python -I -B tools/loginfixture/prepare_safe_login.py --selftest

退出码：0=GREEN（dry-run 或应用并校验通过）/ 1=RED（应用失败或改后校验不符）/
        2=PREREQ（库名守卫、参数非法、角色 0 行/多行、缺 ablity 行、mysql 调用失败）。

本车道（tier=L，tools/）**没有**在真机窗口里跑过本脚本；它只证明「夹具与守卫逻辑会咬人」，
真机证据必须由窗口执行人按 `tools/loginfixture/README.md` 的步骤采集。
"""
import argparse
import subprocess
import sys

DEFAULT_MYSQL = r"D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe"
DB_PREFIX = "mir2_test_"
SAFE_MAP = "0"
SAFE_X = 289
SAFE_Y = 618
LEVEL_MAX = 255          # characters_ablity.Level = tinyint unsigned
HP_MAX = 32767           # Hp/MaxHP = smallint signed
COORD_MAX = 1000000


class Prereq(Exception):
    """前置不满足：rc=2，不写库。"""


class Red(Exception):
    """判据不成立：rc=1。"""


def db_guard(db):
    """库名守卫：只允许 mir2_test_*（前缀后至少一个字符），并拒绝反引号/NUL。"""
    if not isinstance(db, str) or not db.startswith(DB_PREFIX) or len(db) == len(DB_PREFIX):
        raise Prereq("库名守卫：只允许 mir2_test_*（收到 %r）" % (db,))
    if "`" in db or "\x00" in db:
        raise Prereq("库名守卫：库名含反引号/NUL（收到 %r）" % (db,))
    return db


def quote_ident(name):
    if not name or "`" in name or "\x00" in name:
        raise Prereq("非法标识符 %r" % (name,))
    return "`" + name + "`"


def sql_quote(value):
    """MySQL 单引号字符串（默认 sql_mode，反斜杠是转义符）：先转义 \\，再转义 '。"""
    s = str(value)
    if "\x00" in s:
        raise Prereq("字符串含 NUL，拒绝拼 SQL")
    return "'" + s.replace("\\", "\\\\").replace("'", "''") + "'"


def check_values(args):
    if not (0 <= args.level <= LEVEL_MAX):
        raise Prereq("--level 必须在 0..%d（收到 %d）" % (LEVEL_MAX, args.level))
    if not (0 <= args.hp <= HP_MAX):
        raise Prereq("--hp 必须在 0..%d（收到 %d）" % (HP_MAX, args.hp))
    if not (0 <= args.max_hp <= HP_MAX):
        raise Prereq("--max-hp 必须在 0..%d（收到 %d）" % (HP_MAX, args.max_hp))
    if args.hp > args.max_hp:
        raise Prereq("--hp(%d) > --max-hp(%d)：拒绝制造非法角色行" % (args.hp, args.max_hp))
    if not (0 <= args.x <= COORD_MAX and 0 <= args.y <= COORD_MAX):
        raise Prereq("--x/--y 必须在 0..%d" % COORD_MAX)
    if not args.map:
        raise Prereq("--map 不能为空")
    if (args.char is None) == (args.char_id is None):
        raise Prereq("--char 与 --char-id 必须且只能给一个")
    if args.char_id is not None and args.char_id <= 0:
        raise Prereq("--char-id 必须是正整数")


def resolve_sql(args):
    db = quote_ident(args.db)
    if args.char_id is not None:
        return ("SELECT `Id`, `ChrName` FROM %s.`characters` WHERE `Id`=%d;"
                % (db, args.char_id))
    return ("SELECT `Id`, `ChrName` FROM %s.`characters` WHERE `ChrName`=%s;"
            % (db, sql_quote(args.char)))


def snapshot_sql(args, char_id):
    db = quote_ident(args.db)
    return (
        "SELECT 'characters', `Id`, `ChrName`, `MapName`, `CX`, `CY`, `Level`, `Dir`, `Gold`, `Job` "
        "FROM %s.`characters` WHERE `Id`=%d;\n"
        "SELECT 'characters_ablity', `PlayerId`, `Level`, `Hp`, `Mp`, `MaxHP`, `MAxMP`, `Exp`, `MaxExp`, `Weight`, `MaxWeight` "
        "FROM %s.`characters_ablity` WHERE `PlayerId`=%d;\n"
        % (db, char_id, db, char_id)
    )


def apply_sql(args, char_id):
    db = quote_ident(args.db)
    touch_char = ", `LASTUPDATE`=NOW()" if args.touch_timestamps else ""
    touch_abil = ", `ModifyTime`=NOW()" if args.touch_timestamps else ""
    return (
        "START TRANSACTION;\n"
        "UPDATE %s.`characters` SET `MapName`=%s, `CX`=%d, `CY`=%d, `Level`=%d%s WHERE `Id`=%d;\n"
        "UPDATE %s.`characters_ablity` SET `Level`=%d, `Hp`=%d, `MaxHP`=%d%s WHERE `PlayerId`=%d;\n"
        "COMMIT;\n"
        % (db, sql_quote(args.map), args.x, args.y, args.level, touch_char, char_id,
           db, args.level, args.hp, args.max_hp, touch_abil, char_id)
    )


def mysql_cmd(args, sql):
    """mysql 客户端命令。`--skip-column-names` 是硬要求：否则 SELECT 的表头会被当成数据行。"""
    return [
        args.mysql,
        "--batch",
        "--skip-column-names",
        "--default-character-set=" + args.charset,
        "-h", args.host,
        "-P", str(args.port),
        "-u", args.user,
        "--password=" + args.password,
        "-e", sql,
    ]


def run_mysql(args, sql, runner=None):
    """返回 (rc, stdout, stderr)。runner 仅供 --selftest 注入。"""
    if runner is not None:
        return runner(args, sql)
    cmd = mysql_cmd(args, sql)
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=args.timeout)
    except OSError as e:
        raise Prereq("无法执行 mysql 客户端 %s：%s" % (args.mysql, e))
    return r.returncode, r.stdout.decode("utf-8", "replace"), r.stderr.decode("utf-8", "replace")


def parse_tsv(out):
    rows = []
    for line in out.splitlines():
        if line == "":
            continue
        rows.append(line.split("\t"))
    return rows


def snapshot_view(rows):
    char = next((r for r in rows if r and r[0] == "characters"), None)
    abil = next((r for r in rows if r and r[0] == "characters_ablity"), None)
    return char, abil


def snapshot_text(label, char, abil):
    lines = ["%s:" % label]
    if char is None:
        lines.append("  characters: <no row>")
    else:
        lines.append("  characters        Id=%s ChrName=%s MapName=%s CX=%s CY=%s Level=%s Dir=%s Gold=%s Job=%s"
                     % tuple(char[1:10]))
    if abil is None:
        lines.append("  characters_ablity: <no row>")
    else:
        lines.append("  characters_ablity PlayerId=%s Level=%s Hp=%s Mp=%s MaxHP=%s MAxMP=%s Exp=%s MaxExp=%s Weight=%s MaxWeight=%s"
                     % tuple(abil[1:11]))
    return "\n".join(lines)


def verify_after(args, char, abil, char_id):
    problems = []
    if char is None or abil is None:
        return ["改后快照缺行（characters=%r, characters_ablity=%r）"
                % (char is not None, abil is not None)]
    try:
        if char[1] != str(char_id):
            problems.append("characters.Id=%s != 目标 %d" % (char[1], char_id))
        if abil[1] != str(char_id):
            problems.append("characters_ablity.PlayerId=%s != 目标 %d" % (abil[1], char_id))
        if char[3] != str(args.map):
            problems.append("characters.MapName=%r != %r" % (char[3], args.map))
        if int(char[4]) != args.x:
            problems.append("characters.CX=%s != %d" % (char[4], args.x))
        if int(char[5]) != args.y:
            problems.append("characters.CY=%s != %d" % (char[5], args.y))
        if int(char[6]) != args.level:
            problems.append("characters.Level=%s != %d" % (char[6], args.level))
        if int(abil[2]) != args.level:
            problems.append("characters_ablity.Level=%s != %d" % (abil[2], args.level))
        if int(abil[3]) != args.hp:
            problems.append("characters_ablity.Hp=%s != %d" % (abil[3], args.hp))
        if int(abil[5]) != args.max_hp:
            problems.append("characters_ablity.MaxHP=%s != %d" % (abil[5], args.max_hp))
    except (IndexError, ValueError) as e:
        problems.append("改后快照列数/数值异常：%s" % e)
    return problems


def run(args, runner=None):
    db_guard(args.db)
    check_values(args)

    rc, out, err = run_mysql(args, resolve_sql(args), runner=runner)
    if rc != 0:
        raise Prereq("解析角色失败（mysql rc=%d）：%s" % (rc, err.strip()[:300]))
    targets = parse_tsv(out)
    if len(targets) != 1:
        raise Prereq("角色定位必须恰好 1 行（收到 %d 行）；用 --char 或 --char-id 精确指定"
                     % len(targets))
    char_id = int(targets[0][0])
    chr_name = targets[0][1]

    rc, out, err = run_mysql(args, snapshot_sql(args, char_id), runner=runner)
    if rc != 0:
        raise Prereq("读取改动前快照失败（mysql rc=%d）：%s" % (rc, err.strip()[:300]))
    before_char, before_abil = snapshot_view(parse_tsv(out))
    if before_char is None:
        raise Prereq("characters 里找不到 Id=%d" % char_id)
    if before_abil is None:
        raise Prereq("characters_ablity 里找不到 PlayerId=%d（不新建行，先确认角色有 ablity 行）"
                     % char_id)
    if before_char[1] != str(char_id) or before_abil[1] != str(char_id):
        raise Prereq("改前快照的 Id/PlayerId 与目标 %d 不一致（查询/连接串可疑）" % char_id)

    print("TARGET db=%s char_id=%d chr_name=%s" % (args.db, char_id, chr_name))
    print(snapshot_text("BEFORE", before_char, before_abil))
    print("PLAN:")
    print(apply_sql(args, char_id).rstrip())

    if not args.yes_apply:
        print("DRY-RUN: 未写库；确认无误后加 --yes-apply 再跑一次。")
        return 0

    rc, out, err = run_mysql(args, apply_sql(args, char_id), runner=runner)
    if rc != 0:
        raise Red("应用 UPDATE 失败（mysql rc=%d）：%s" % (rc, err.strip()[:300]))

    rc, out, err = run_mysql(args, snapshot_sql(args, char_id), runner=runner)
    if rc != 0:
        raise Red("应用后复读快照失败（mysql rc=%d）：%s" % (rc, err.strip()[:300]))
    after_char, after_abil = snapshot_view(parse_tsv(out))
    print(snapshot_text("AFTER", after_char, after_abil))

    problems = verify_after(args, after_char, after_abil, char_id)
    if problems:
        raise Red("改后校验不符：\n  - " + "\n  - ".join(problems))
    print("GREEN: 角色已摆到 map=%s (%d,%d), Level=%d, Hp=%d/%d"
          % (args.map, args.x, args.y, args.level, args.hp, args.max_hp))
    return 0


# ---------------------------------------------------------------- selftest

BEFORE_CHAR = "characters\t6228\twinplay0\t3\t324\t327\t40\t4\t0\t1"
BEFORE_ABIL = "characters_ablity\t6228\t40\t674\t151\t674\t151\t15689\t10000000\t430\t570"
AFTER_CHAR = "characters\t6228\twinplay0\t0\t289\t618\t1\t4\t0\t1"
AFTER_ABIL = "characters_ablity\t6228\t1\t19\t151\t19\t151\t15689\t10000000\t430\t570"


class FakeDB(object):
    """只认本脚本会发出的三类 SQL；够用来测守卫/事务/改后校验，不模拟真 MySQL。"""

    def __init__(self, after_char=AFTER_CHAR, after_abil=AFTER_ABIL, resolve_rows=None,
                 apply_rc=0, before_abil=BEFORE_ABIL):
        self.after_char = after_char
        self.after_abil = after_abil
        self.resolve_rows = resolve_rows if resolve_rows is not None else ["6228\twinplay0"]
        self.apply_rc = apply_rc
        self.before_abil = before_abil
        self.applied = False
        self.apply_calls = 0
        self.sqls = []
        self.missing_update = False

    def __call__(self, args, sql):
        self.sqls.append(sql)
        if sql.startswith("SELECT `Id`, `ChrName`"):
            return 0, "\n".join(self.resolve_rows) + ("\n" if self.resolve_rows else ""), ""
        if sql.startswith("SELECT 'characters'"):
            if self.applied:
                lines = [self.after_char]
                if self.after_abil is not None:
                    lines.append(self.after_abil)
                return 0, "\n".join(lines) + "\n", ""
            lines = [BEFORE_CHAR]
            if self.before_abil is not None:
                lines.append(self.before_abil)
            return 0, "\n".join(lines) + "\n", ""
        if sql.startswith("START TRANSACTION"):
            self.apply_calls += 1
            if self.apply_rc != 0:
                return self.apply_rc, "", "fake mysql failure"
            if "UPDATE `mir2_test_db`.`characters` SET" not in sql:
                self.missing_update = True
            if "UPDATE `mir2_test_db`.`characters_ablity` SET" not in sql:
                self.missing_update = True
            self.applied = True
            return 0, "", ""
        raise AssertionError("FakeDB 不认识这条 SQL：%r" % sql[:160])


def _base_argv(*extra):
    return [
        "--db", "mir2_test_db", "--char", "winplay0",
        "--level", "1", "--hp", "19", "--max-hp", "19",
    ] + list(extra)


def selftest():
    ok = True

    def expect(name, cond, detail=""):
        nonlocal ok
        good = bool(cond)
        ok = ok and good
        print("%-64s %s%s" % (name, "OK" if good else "FAIL",
                              (" " + detail) if (detail and not good) else ""))

    def rc_of(argv, runner):
        try:
            return run(_args(argv), runner=runner), ""
        except Prereq as e:
            return 2, str(e)
        except Red as e:
            return 1, str(e)

    # 1) 库名守卫：只放行 mir2_test_<非空>
    for db in ("mir2_test_db", "mir2_test_account", "mir2_test_data", "mir2_test_x1"):
        expect("db_guard 放行 %s" % db, db_guard(db) == db)
    for db in ("mir2_db", "mir2_account", "mir2_data", "mir2_test", "mir2_test_",
               "", "MIR2_TEST_DB", "mir2_test_db`"):
        try:
            db_guard(db)
            expect("db_guard 拒绝 %r" % db, False, "却放行了")
        except Prereq:
            expect("db_guard 拒绝 %r" % db, True)

    # 2) SQL 转义：引号与反斜杠都不得逃逸
    expect("sql_quote 引号/反斜杠转义",
           sql_quote("O'Brien\\x") == "'O''Brien\\\\x'", sql_quote("O'Brien\\x"))

    # 3) 参数边界
    bad = [
        ("level", ["--level", "256"]),
        ("hp-max", ["--hp", "20", "--max-hp", "19"]),
        ("hp-neg", ["--hp", "-1"]),
        ("coord", ["--x", "-1"]),
    ]
    for name, extra in bad:
        fake = FakeDB()
        r, detail = rc_of(_base_argv(*extra), fake)
        expect("参数边界拒绝 %s ⇒ rc=2" % name, r == 2, str((r, detail)))
        expect("参数边界拒绝 %s 时不触库" % name, fake.sqls == [])

    # 4) dry-run：只读不写
    fake = FakeDB()
    rc = run(_args(_base_argv()), runner=fake)
    expect("dry-run rc=0", rc == 0, str(rc))
    expect("dry-run 不执行事务", fake.apply_calls == 0, str(fake.apply_calls))

    # 5) apply：写两表 + 改后校验通过
    fake = FakeDB()
    rc = run(_args(_base_argv("--yes-apply")), runner=fake)
    expect("apply rc=0", rc == 0, str(rc))
    expect("apply 恰好一次事务", fake.apply_calls == 1, str(fake.apply_calls))
    expect("apply 两条 UPDATE 都在", not fake.missing_update)

    # 6) 改后校验必须咬人：HP 不符 ⇒ RED
    fake = FakeDB(after_abil="characters_ablity\t6228\t1\t20\t151\t19\t151\t0\t10\t0\t50")
    r, detail = rc_of(_base_argv("--yes-apply"), fake)
    expect("改后 Hp 不符 ⇒ RED", r == 1, str((r, detail)))

    # 7) 角色 0 行 / 多行 ⇒ PREREQ
    for name, rows in (("0 行", []), ("多行", ["6228\twinplay0", "6229\twinplay0"])):
        fake = FakeDB(resolve_rows=rows)
        r, detail = rc_of(_base_argv("--yes-apply"), fake)
        expect("角色%s ⇒ PREREQ" % name, r == 2, str((r, detail)))
        expect("角色%s 时不触写" % name, fake.apply_calls == 0)

    # 8) 缺 ablity 行 ⇒ PREREQ（不新建）
    fake = FakeDB(before_abil=None)
    r, detail = rc_of(_base_argv("--yes-apply"), fake)
    expect("缺 characters_ablity 行 ⇒ PREREQ", r == 2, str((r, detail)))

    # 9) 库名守卫在 CLI 层先拦：非 test 库不触库
    fake = FakeDB()
    r, detail = rc_of(["--db", "mir2_db", "--char", "winplay0",
                       "--level", "1", "--hp", "19", "--max-hp", "19"], fake)
    expect("CLI 非 mir2_test_* ⇒ rc=2", r == 2, str((r, detail)))
    expect("CLI 非 mir2_test_* 不触库", fake.sqls == [])

    # 10) 默认安全格与 SQL 结构
    sql = apply_sql(_args(_base_argv()), 6228)
    expect("默认安全格 (289,618) / map 0",
           "`CX`=289" in sql and "`CY`=618" in sql and "`MapName`='0'" in sql, sql)
    expect("两表都被 UPDATE", "`characters` SET" in sql and "`characters_ablity` SET" in sql, sql)
    expect("包在单事务里", sql.startswith("START TRANSACTION;") and "COMMIT;" in sql, sql)
    expect("mysql 命令跳过列名（否则表头会被当数据行）",
           "--skip-column-names" in mysql_cmd(_args(_base_argv()), "SELECT 1"), mysql_cmd(_args(_base_argv()), "SELECT 1"))

    # 11) --char-id 路径
    fake = FakeDB(resolve_rows=["6228\twinplay0"])
    rc = run(_args(["--db", "mir2_test_db", "--char-id", "6228",
                    "--level", "1", "--hp", "19", "--max-hp", "19"]), runner=fake)
    expect("--char-id 可用", rc == 0 and any("`Id`=6228" in s for s in fake.sqls), str(rc))

    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def _args(argv):
    return build_parser().parse_args(argv)


def build_parser():
    ap = argparse.ArgumentParser(description="窗口受控登录夹具：把 mir2_test_* 角色摆到安全格")
    ap.add_argument("--db", default="mir2_test_db", help="只允许 mir2_test_*（守卫强制）")
    ap.add_argument("--char", default=None, help="角色名（与 --char-id 二选一）")
    ap.add_argument("--char-id", type=int, default=None, help="characters.Id（与 --char 二选一）")
    ap.add_argument("--map", default=SAFE_MAP)
    ap.add_argument("--x", type=int, default=SAFE_X)
    ap.add_argument("--y", type=int, default=SAFE_Y)
    ap.add_argument("--level", type=int, default=None)
    ap.add_argument("--hp", type=int, default=None)
    ap.add_argument("--max-hp", type=int, default=None, dest="max_hp")
    ap.add_argument("--touch-timestamps", action="store_true",
                    help="同时把 LASTUPDATE/ModifyTime 设为 NOW()（默认不改）")
    ap.add_argument("--yes-apply", action="store_true", help="真正写库（默认 dry-run）")
    ap.add_argument("--mysql", default=DEFAULT_MYSQL)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=3306)
    ap.add_argument("--user", default="root")
    ap.add_argument("--password", default="")
    ap.add_argument("--charset", default="utf8mb4")
    ap.add_argument("--timeout", type=int, default=60)
    ap.add_argument("--selftest", action="store_true")
    return ap


def main(argv=None, runner=None):
    args = build_parser().parse_args(argv)
    if args.selftest:
        return selftest()
    if args.level is None or args.hp is None or args.max_hp is None:
        print("PREREQ_MISSING: 必须给 --level/--hp/--max-hp（本工具不猜数值）", file=sys.stderr)
        return 2
    try:
        return run(args, runner=runner)
    except Prereq as e:
        print("PREREQ: %s" % e, file=sys.stderr)
        return 2
    except Red as e:
        print("RED: %s" % e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
