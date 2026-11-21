//! 卡 `openmir2-magic-serverconf-batch-plan` §6.7：**Server.conf 解析对拍照**（独立验收项）。
//!
//! C# `ServerConf.LoadConfig()`（探针 `tests/parity/csharp/ServerConfProbe`）与 Rust
//! `game_svr::config::load` 对**同一份 ini** 的六个契约键解析值必须一致：
//!   `MagTurnUndeadLevel` / `MabMabeHitRandRate` / `MabMabeHitMinLvLimit` /
//!   `MabMabeHitSucessRate` / `MabMabeHitMabeTimeRate` / `AmyOunsulPoint`。
//!
//! 合成夹具（`tests/parity/fixtures/serverconf/`，随仓提交）覆盖并**断言绝对期望值**：
//!   ① 六键齐全 = 部署值 44/100/10/21/20/15；
//!   ② 逐键缺失 ⇒ 该键回落（C# ctor 缺省 / Rust `unwrap_or`），其余保持部署值；
//!   ③ 某键 = 0；④ 某键 = 负值 / 非数字（非数字按**已知不对称**断言：C# 回落缺省、Rust 报错，
//!   见 `docs/issues/openmir2-serverconf-nonnumeric-asymmetry.md`）。
//! 任何一侧语义漂移（含"把那一个键从 44 改成 50"这种基线改动）都会因**期望值断言**变红。
//!
//! 真实部署文件（`E:/MirServer/M2GameSvr/Server.conf`）**只在存在时**验：断言六键 == 部署值并记录
//! sha256；缺席时其余用例照样全绿。
//!
//! 本测试**不进 magic 驱动、不作为 golden 生成前提**（golden 仍是可移植冻结件）。

use std::path::{Path, PathBuf};
use std::process::Command;

const KEYS: [&str; 6] = [
    "mag_turn_undead_level",
    "mab_mabe_hit_rand_rate",
    "mab_mabe_hit_min_lv_limit",
    "mab_mabe_hit_success_rate",
    "mab_mabe_hit_mabe_time_rate",
    "amy_ounsul_point",
];

/// 部署口径（`Server.conf`）的期望值。
const DEPLOY: [i32; 6] = [44, 100, 10, 21, 20, 15];
/// C# `GameSvrConf` ctor 缺省 / Rust `unwrap_or` 缺省。
const CTOR_DEFAULTS: [i32; 6] = [50, 100, 10, 21, 20, 10];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/fixtures/serverconf")
}

fn probe_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/csharp/ServerConfProbe")
}

/// 构建（如需要）并返回探针 exe。
fn ensure_probe() -> PathBuf {
    let dir = probe_dir();
    let csproj = dir.join("ServerConfProbe.csproj");
    let exe = dir.join("bin/Release/ServerConfProbe.exe");
    if exe.exists() {
        return exe;
    }
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

fn rust_values(conf: &Path) -> Result<Vec<i32>, String> {
    let c = game_svr::config::load(conf).map_err(|e| format!("{e:#}"))?;
    Ok(vec![
        c.mag_turn_undead_level,
        c.mab_mabe_hit_rand_rate,
        c.mab_mabe_hit_min_lv_limit,
        c.mab_mabe_hit_sucess_rate,
        c.mab_mabe_hit_mabe_time_rate,
        c.amy_ounsul_point,
    ])
}

/// 两侧都必须等于**绝对期望值**，且相互相等。
fn assert_case(name: &str, expected: [i32; 6]) {
    let path = fixture_dir().join(name);
    let c = csharp_values(&path);
    assert_eq!(c, expected.to_vec(), "[{name}] C# 侧解析值与期望值不一致");
    let r = rust_values(&path).unwrap_or_else(|e| panic!("[{name}] Rust 侧解析失败：{e}"));
    assert_eq!(r, expected.to_vec(), "[{name}] Rust 侧解析值与期望值不一致");
    assert_eq!(c, r, "[{name}] 两侧解析值不一致");
    eprintln!("CASE {name}: csharp={c:?} rust={r:?} expected={expected:?}");
}

#[test]
fn synthetic_serverconf_parses_identically() {
    // ① 六键齐全 = 部署值
    assert_case("deploy.conf", DEPLOY);

    // ② 逐键缺失 ⇒ 该键回落 ctor 缺省，其余保持部署值
    let mut with_default = DEPLOY;
    with_default[0] = CTOR_DEFAULTS[0];
    assert_case("missing_magturnundeadlevel.conf", with_default);
    // 其余四项的缺省 == 部署值（100/10/21/20），缺失后向量不变
    assert_case("missing_mabmabehitrandrate.conf", DEPLOY);
    assert_case("missing_mabmabehitminlvlimit.conf", DEPLOY);
    assert_case("missing_mabmabehitsucessrate.conf", DEPLOY);
    assert_case("missing_mabmabehitmabetimerate.conf", DEPLOY);
    let mut with_default_amy = DEPLOY;
    with_default_amy[5] = CTOR_DEFAULTS[5];
    assert_case("missing_amyounsulpoint.conf", with_default_amy);

    // ③ 某键 = 0
    let mut zero = DEPLOY;
    zero[0] = 0;
    assert_case("zero.conf", zero);

    // ④ 某键为负 / 非数字
    let mut negative = DEPLOY;
    negative[5] = -5;
    assert_case("negative.conf", negative);

    // 空值（`KEY=`）：只对带 `!v.is_empty()` 守卫的键两侧一致——
    // Rust `MagTurnUndeadLevel` 有守卫（`config.rs` 的 `MAGTURNUNDEADLEVEL` 臂）、C# `TryParse("")`
    // 失败回落 ⇒ 两侧都应回落 ctor 缺省 50。注：`MabMabe*`/`AmyOunsulPoint` 的 Rust 臂**无**守卫，
    // 空值会走 parse Err（与非法数字同类，见下面 nonnumeric 与
    // `docs/issues/openmir2-serverconf-nonnumeric-asymmetry.md`）。
    let mut empty = DEPLOY;
    empty[0] = CTOR_DEFAULTS[0];
    assert_case("empty.conf", empty);

    // 已知不对称（显式登记，不当作红）：非数字时两侧语义不同——
    //   C# `ConfigFile.ReadWriteInteger`/`ReadInteger`：键存在但 `int.TryParse` 失败 ⇒ 回落缺省
    //     （`src/OpenMir2/Common/ConfigFile.cs` 的 `ReadWriteInteger` 与 `ReadInteger`）；
    //   Rust `config.rs` 的 `MABMABEHITRANDRATE` 臂：`v.parse().with_context(...)?` ⇒ 返回 Err。
    // 这是**启动期解析语义**，不产生帧面差异；当前决定=保留更严格的 Rust 行为并登记为偏差
    // （卡 `openmir2-serverconf-nonnumeric-asymmetry`，含移除条件）。
    {
        let path = fixture_dir().join("nonnumeric.conf");
        let c = csharp_values(&path);
        assert_eq!(
            c,
            DEPLOY.to_vec(),
            "[nonnumeric.conf] C# 侧必须回落缺省（MabMabeHitRandRate=100）"
        );
        let r = rust_values(&path);
        assert!(
            r.is_err(),
            "[nonnumeric.conf] Rust 侧应显式报错（Got {r:?}）——本用例钉的是已知不对称，不是值相等"
        );
        eprintln!(
            "CASE nonnumeric.conf: csharp={c:?} rust=Err({}) expected=已知不对称",
            r.unwrap_err()
        );
    }
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
        "真实部署 Server.conf 的六个契约键必须等于部署口径（44/100/10/21/20/15）"
    );
}
