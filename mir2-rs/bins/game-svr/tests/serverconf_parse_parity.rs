//! 卡 `openmir2-magic-serverconf-batch-plan` §6.7：**Server.conf 解析对拍照**（独立验收项）。
//!
//! 覆盖**两个 Rust 解析器**与 oracle 的逐键对拍：
//!   1. `game_svr::config::load`（`bins/game-svr/src/config.rs`）——圣言/魔爆六键：
//!      `MagTurnUndeadLevel` / `MabMabeHitRandRate` / `MabMabeHitMinLvLimit` /
//!      `MabMabeHitSucessRate` / `MabMabeHitMabeTimeRate` / `AmyOunsulPoint`；
//!   2. `mir2_data::pet_slave::PetSlaveConfig::load`（`crates/data/src/pet_slave.rs`）——诱惑之光四键：
//!      `MagTammingLevel` / `MagTammingTargetLevel` / `MagTammingTargetHPRate` / `MagTammingCount`
//!      （注意 oracle 缺陷：HP 率的**键**写作 `MagTammingTargetHPRate`，`ServerConf.cs:533`）。
//!
//! C# 侧由探针 `tests/parity/csharp/ServerConfProbe`（只输出解析值）给出。
//!
//! 合成夹具（`tests/parity/fixtures/serverconf/`，随仓提交）覆盖并**断言绝对期望值**：
//!   ① 全键 = 部署值 `44/100/10/21/20/15` + `40/1/100/5`（**四条缺省≠部署**：44/15/40/1）；
//!   ② 逐键缺失 ⇒ 该键回落缺省，其余保持部署值；
//!   ③ 某键 = 0；④ 某键为负 / 空 / 非数字。
//! 非数字与「无守卫键的空值」按**已知不对称**断言（C# 回落缺省、Rust `config.rs` 返回 Err），
//! 见 `docs/issues/openmir2-serverconf-nonnumeric-asymmetry.md`；`PetSlaveConfig` 的
//! `read_integer` 对空/非数字都回落缺省（`unwrap_or(def)`），**不属于**不对称。
//!
//! 真实部署文件（`E:/MirServer/M2GameSvr/Server.conf`）**只在存在时**验：断言十键 == 部署值并记录
//! sha256；缺席时其余用例照样全绿。
//!
//! 本测试**不进 magic 驱动、不作为 golden 生成前提**（golden 仍是可移植冻结件）。

use std::path::{Path, PathBuf};
use std::process::Command;

const KEYS: [&str; 10] = [
    "mag_turn_undead_level",
    "mab_mabe_hit_rand_rate",
    "mab_mabe_hit_min_lv_limit",
    "mab_mabe_hit_sucess_rate",
    "mab_mabe_hit_mabe_time_rate",
    "amy_ounsul_point",
    "mag_tamming_level",
    "mag_tamming_target_level",
    "mag_tamming_hp_rate",
    "mag_tamming_count",
];

/// 部署口径（`Server.conf`）的期望值。
const DEPLOY: [i32; 10] = [44, 100, 10, 21, 20, 15, 40, 1, 100, 5];
/// C# `GameSvrConf` ctor 缺省 / Rust 两解析器的缺省。
const CTOR_DEFAULTS: [i32; 10] = [50, 100, 10, 21, 20, 10, 50, 10, 100, 5];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/fixtures/serverconf")
}

fn probe_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/csharp/ServerConfProbe")
}

/// 构建探针并返回 exe；**每个测试进程构建一次**（`OnceLock` 串行化）。
///
/// 两个 `#[test]` 会并行跑；若各自 `dotnet build` 同一工程会抢 MSBuild 文件锁（实测会有一侧
/// 构建失败）。这里用 `OnceLock` 保证进程内只构建一次；跨进程（每次 `cargo test`）都会重新
/// 增量构建 ⇒ 改了 `Program.cs` 不会静默拿旧探针。
fn ensure_probe() -> PathBuf {
    static PROBE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    PROBE.get_or_init(build_probe).clone()
}

fn build_probe() -> PathBuf {
    let dir = probe_dir();
    let csproj = dir.join("ServerConfProbe.csproj");
    let exe = dir.join("bin/Release/ServerConfProbe.exe");
    // **每次都调 `dotnet build`（增量）**：只要探针源变了（例如以后加键），MSBuild 会重编；
    // 不能 `if exe.exists() { return }`——那会在改了 `Program.cs` 却没删 exe 时拿**旧探针**验旧键集，
    // 测试照样绿（假绿）。增量构建通常 1 秒内，正确性优先。
    let out = Command::new("dotnet")
        .arg("build")
        .arg(&csproj)
        .args(["-c", "Release", "-v", "q", "--nologo"])
        .output()
        .expect("启动 `dotnet build` 失败（需要 dotnet 在 PATH）");
    assert!(
        out.status.success(),
        "dotnet build 探针失败：\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        exe.exists(),
        "dotnet build 后仍找不到探针 exe：{}",
        exe.display()
    );
    exe
}

fn parse_probe_output(text: &str) -> Vec<i32> {
    let mut v = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, val) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("探针输出行没有 '='：{line:?}"));
        if !KEYS.contains(&k) {
            continue;
        }
        v.push(
            val.trim()
                .parse()
                .unwrap_or_else(|_| panic!("探针输出不是整数：{line:?}")),
        );
    }
    assert_eq!(v.len(), KEYS.len(), "探针输出缺键：\n{text}");
    v
}

fn csharp_values(conf: &Path) -> Vec<i32> {
    let exe = ensure_probe();
    let out = Command::new(&exe)
        .arg(conf)
        .output()
        .expect("启动 C# 探针失败");
    assert!(
        out.status.success(),
        "C# 探针 rc≠0（{}）：{}",
        conf.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_probe_output(&String::from_utf8_lossy(&out.stdout))
}

/// 两个 Rust 解析器的十键合并视图（前者失败即整体失败）。
fn rust_values(conf: &Path) -> Result<Vec<i32>, String> {
    let c = game_svr::config::load(conf).map_err(|e| format!("{e:#}"))?;
    let p = mir2_data::pet_slave::PetSlaveConfig::load(conf)
        .map_err(|e| format!("PetSlaveConfig::load: {e}"))?;
    Ok(vec![
        c.mag_turn_undead_level,
        c.mab_mabe_hit_rand_rate,
        c.mab_mabe_hit_min_lv_limit,
        c.mab_mabe_hit_sucess_rate,
        c.mab_mabe_hit_mabe_time_rate,
        c.amy_ounsul_point,
        p.mag_tamming_level,
        p.mag_tamming_target_level,
        p.mag_tamming_hp_rate,
        i32::from(p.mag_tamming_count),
    ])
}

fn with(slot: usize, value: i32) -> [i32; 10] {
    let mut e = DEPLOY;
    e[slot] = value;
    e
}

/// 两侧都必须等于**绝对期望值**，且相互相等。
fn assert_case(name: &str, expected: [i32; 10]) {
    let path = fixture_dir().join(name);
    let c = csharp_values(&path);
    assert_eq!(c, expected.to_vec(), "[{name}] C# 侧解析值与期望值不一致");
    let r = rust_values(&path).unwrap_or_else(|e| panic!("[{name}] Rust 侧解析失败：{e}"));
    assert_eq!(r, expected.to_vec(), "[{name}] Rust 侧解析值与期望值不一致");
    assert_eq!(c, r, "[{name}] 两侧解析值不一致");
    eprintln!("CASE {name}: csharp={c:?} rust={r:?} expected={expected:?}");
}

/// 已知不对称：C# 侧必须等于期望值（回落缺省），Rust 侧必须显式 `Err`。
/// 依据：C# `ConfigFile.ReadWriteInteger`/`ReadInteger`（`src/OpenMir2/Common/ConfigFile.cs:55-63`、
/// `:170-185`）对空/非数字回落缺省；Rust `config.rs` 的 `MABMABEHITRANDRATE` 等臂
/// `v.parse().with_context(...)?` 直接返回错误。详见
/// `docs/issues/openmir2-serverconf-nonnumeric-asymmetry.md`。
fn assert_known_asymmetry(name: &str) {
    let path = fixture_dir().join(name);
    let c = csharp_values(&path);
    assert_eq!(
        c,
        DEPLOY.to_vec(),
        "[{name}] C# 侧必须回落缺省（其它键保持部署值）"
    );
    let r = rust_values(&path);
    assert!(
        r.is_err(),
        "[{name}] Rust 侧应显式报错（Got {r:?}）——本用例钉的是已知不对称，不是值相等"
    );
    eprintln!(
        "CASE {name}: csharp={c:?} rust=Err({}) expected=已知不对称",
        r.unwrap_err()
    );
}

#[test]
fn synthetic_serverconf_parses_identically() {
    // ① 全键 = 部署值
    assert_case("deploy.conf", DEPLOY);

    // ② 逐键缺失 ⇒ 该键回落缺省，其余保持部署值
    for slot in 0..KEYS.len() {
        let name = format!("missing_{}.conf", KEYS[slot]);
        assert_case(&name, with(slot, CTOR_DEFAULTS[slot]));
    }

    // ③ 某键 = 0（圣言门 / 诱惑等级）
    assert_case("zero.conf", with(0, 0));
    assert_case("pet_zero.conf", with(6, 0));

    // ④ 负值
    assert_case("negative.conf", with(5, -5));
    assert_case("pet_negative.conf", with(7, -3));

    // 空值：带 `!v.is_empty()` 守卫的键（`MagTurnUndeadLevel`）两侧都回落缺省。
    assert_case("empty.conf", with(0, CTOR_DEFAULTS[0]));
    // 空值：`MagTammingLevel` 无守卫但 `IniFile::read_integer` 对空回落 `def`（`unwrap_or`）⇒ 两侧一致。
    assert_case("pet_empty.conf", with(6, CTOR_DEFAULTS[6]));
    // 非数字：`PetSlaveConfig` 的 `read_integer` 对非数字回落 `def` ⇒ 两侧一致（与 config.rs 不同）。
    assert_case("pet_nonnumeric.conf", with(6, CTOR_DEFAULTS[6]));

    // 已知不对称（显式登记，不当作红）：
    //   - 非数字：C# 回落缺省（100），Rust `config.rs` 返回 Err；
    //   - 空值（无守卫键 `MabMabeHitRandRate=`）：同上。
    assert_known_asymmetry("nonnumeric.conf");
    assert_known_asymmetry("empty_mabmabe.conf");
}

/// 真实部署文件**只在存在时**验；缺席不影响上面的合成用例。
#[test]
fn real_deployment_conf_matches_when_present() {
    let path = Path::new("E:/MirServer/M2GameSvr/Server.conf");
    if !path.exists() {
        eprintln!(
            "SKIP 真实部署 Server.conf 不存在：{}（合成用例不依赖它）",
            path.display()
        );
        return;
    }

    let bytes = std::fs::read(path).expect("读真实 Server.conf 失败");
    use sha2::{Digest, Sha256};
    let sha = format!("{:x}", Sha256::digest(&bytes));
    eprintln!("REAL ServerConf sha256={sha}");

    let c = csharp_values(path);
    let r = rust_values(path).unwrap_or_else(|e| panic!("Rust 解析真实 Server.conf 失败：{e}"));
    assert_eq!(c, r, "真实部署文件：两侧解析值不一致");
    assert_eq!(
        c,
        DEPLOY.to_vec(),
        "真实部署 Server.conf 的十个契约键必须等于部署口径（44/100/10/21/20/15 + 40/1/100/5）"
    );
}
