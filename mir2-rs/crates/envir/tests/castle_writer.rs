//! A-22 P2 `castle-persist-writer` 的构造级判据 C-A22-3..7 + 落盘薄封装。
//!
//! oracle 锚：
//! - `ConfigFile.Load`：`src/OpenMir2/Common/ConfigFile.cs:256-346`
//! - `ConfigFile.Save`：`ConfigFile.cs:348-380`
//! - `CastleConf.LoadConfig` / `SaveConfig`：`src/M2Server/Castle/CastleConf.cs:15-94` / `:96-318`
//! - `StringList.SaveToFile`（文件不存在 ⇒ 抛）：`src/OpenMir2/Common/StringList.cs:260-271`
//! - `CastleManager.SaveCastleList`：`CastleManager.cs:178-192`
//!
//! 夹具：`fixtures/SabukW-Castle.hex` = 部署件 `E:/MirServer/M2GameSvr/Castle/Castle/SabukW.txt`
//! 的原始字节（hex 入库，避免 GBK + CRLF 被 git 文本归一改字节）。

use mir2_envir::{
    decode_config_text, parse_ordered, parse_sabuk_w_bytes, serialize_castle_list,
    serialize_sabuk_w, write_attack_sabuk_wall, write_castle_list, write_sabuk_w,
    AttackSabukWallEntry, EnvirError, IniOrdered, SabukWConfig, SabukWDefaults, SabukWOverlay,
    SabukWUnitOverlay,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 部署件 `Castle/Castle/SabukW.txt`（8870 B，131 键，sha256 `b84c2679…`）。
const DEPLOYED_CASTLE_SABUK_W_HEX: &str = include_str!("fixtures/SabukW-Castle.hex");
const DEPLOYED_CASTLE_SABUK_W_SHA256: &str =
    "b84c26790c994ed0c8ab7d569c89f4edfb20f074e33f652995d386421b8dd053";

/// 部署件 `Castle/0/SabukW.txt`（装载城；`ChangeDate=1998-01-01` / `WarDate=1998-01-01`）。
/// 读侧证据夹具在 `mir2-rs/tests/parity/m3/evidence-m3-castle-file-load-20261013/fixtures/`。
const DEPLOYED_ZERO_SABUK_W_HEX: &str = include_str!(
    "../../../tests/parity/m3/evidence-m3-castle-file-load-20261013/fixtures/SabukW-0.hex"
);

fn hex_bytes(hex: &str) -> Vec<u8> {
    let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    assert_eq!(digits.len() % 2, 0, "hex 夹具长度必须为偶");
    digits
        .chunks(2)
        .map(|c| {
            let hi = (c[0] as char).to_digit(16).expect("hex 高位") as u8;
            let lo = (c[1] as char).to_digit(16).expect("hex 低位") as u8;
            (hi << 4) | lo
        })
        .collect()
}

fn deployed_bytes() -> Vec<u8> {
    hex_bytes(DEPLOYED_CASTLE_SABUK_W_HEX)
}

fn text_of(bytes: &[u8]) -> String {
    decode_config_text(bytes)
}

/// 判据 C-A22-3 要「sha256 逐字节不变」；envir 的 dev-dependencies 不在本卡写集
/// （`Cargo.toml` 不动），故测试内自带一份最小 SHA-256。
#[allow(clippy::needless_range_loop)]
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

/// **C-A22-3（R）**：部署 `Castle/Castle/SabukW.txt` 原样读入 + 同值 overlay 写回
/// ⇒ 逐字节（因而 sha256）不变。
///
/// 牙（改坏必红，见证据包 redcheck）：
/// - 段末空行删掉 ⇒ 长度/字节红；
/// - `\r\n` → `\n` ⇒ 字节红；
/// - 键序按字母排 ⇒ 字节红；
/// - 用 `IniFile`（`HashMap`）输出 ⇒ 顺序红。
#[test]
fn c_a22_3_sabuk_w_rewrite_idempotent() {
    let bytes = deployed_bytes();
    assert_eq!(
        sha256_hex(&bytes),
        DEPLOYED_CASTLE_SABUK_W_SHA256,
        "部署件夹具被篡改（应为 Castle/Castle/SabukW.txt 原始字节）"
    );

    let base = parse_ordered(&bytes);
    let sections: Vec<&str> = base.sections().iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(sections, ["Setup", "Defense"]);
    assert_eq!(base.key_count(), 131, "[Setup] 7 + [Defense] 124");

    // 读侧（`IniFile`/`HashMap`）与写侧（`IniOrdered`）两份独立解析互证。
    let cfg = parse_sabuk_w_bytes(&bytes);
    let overlay = SabukWOverlay::from_config(&cfg);
    let out = serialize_sabuk_w(&base, &overlay, &SabukWDefaults::default()).expect("同值回写");

    assert_eq!(out, bytes, "同值回写必须逐字节不变");
    assert_eq!(sha256_hex(&out), DEPLOYED_CASTLE_SABUK_W_SHA256);
}

/// **C-A22-4（L）**：输入只含 `[Setup]` + `CastleName=x` ⇒ 缺键回填写出 **131 键**、
/// `[Defense]` 全默认、`CastleMapList=` 行为空但**键存在**。
///
/// 牙：实现成「只写显式设置过的键」⇒ 键数 ≠ 131 红。
#[test]
fn c_a22_4_sabuk_w_missing_keys_writeback() {
    let base = parse_ordered(b"[Setup]\r\nCastleName=x\r\n");
    let out = serialize_sabuk_w(&base, &SabukWOverlay::default(), &SabukWDefaults::default())
        .expect("缺键回写");
    let text = text_of(&out);

    assert_eq!(out.iter().filter(|b| **b == b'=').count(), 131, "131 个键");
    assert_eq!(parse_ordered(&out).key_count(), 131);
    assert!(text.contains("[Setup]\r\n"));
    assert!(text.contains("[Defense]\r\n"));
    // 显式值保留，缺键按 LoadConfig 默认值回填。
    assert!(text.contains("CastleName=x\r\n"));
    assert!(text.contains("CastleMapList=\r\n"), "空列表也必须有键");
    assert!(text.contains("CastleMap=3\r\n"));
    assert!(text.contains("CastleHomeX=644\r\n"));
    assert!(text.contains("CastlePlaceMap=0150\r\n"));
    assert!(text.contains("CastleSecretMap=D701\r\n"));
    assert!(text.contains("MainDoorOpen=1\r\n"));
    assert!(text.contains("MainDoorHP=2000\r\n"));
    assert!(text.contains("RightWallHP=2000\r\n"));
    // 每段末空行（`ConfigFile.Save` 的 `sw.WriteLine("")`）。
    assert!(text.contains("\r\n\r\n[Defense]\r\n"));
    assert!(out.ends_with(b"\r\n\r\n"), "末段也补空行");
}

/// **C-A22-5（L）**：重复段 ⇒ 停止解析（`goto Label_02AE`），其后的 `[Defense]` 不参与解析。
///
/// 牙：允许重复段 ⇒ 解析继续，`CastleHomeX=999` 出现（且 `[Defense]` 出现两次）⇒ 红。
#[test]
fn c_a22_5_sabuk_w_duplicate_section_stops() {
    let src =
        b"[Setup]\r\nCastleName=x\r\n[Setup]\r\nCastleName=y\r\n[Defense]\r\nCastleHomeX=999\r\n";
    let base = parse_ordered(src);
    assert_eq!(base.sections().len(), 1, "重复段 ⇒ 停止解析");
    assert!(!base.contains_section("Defense"));

    let out = serialize_sabuk_w(&base, &SabukWOverlay::default(), &SabukWDefaults::default())
        .expect("缺键回写");
    let text = text_of(&out);
    assert_eq!(text.matches("[Defense]").count(), 1);
    assert_eq!(text.matches("[Setup]").count(), 1);
    assert!(text.contains("CastleName=x\r\n"));
    assert!(!text.contains("CastleName=y"));
    assert!(text.contains("CastleHomeX=644\r\n"), "回填默认值");
    assert!(
        !text.contains("CastleHomeX=999"),
        "第二个 [Defense] 段的行必须被丢弃"
    );
}

/// **C-A22-6（L）**：含中文的输出逐字节等于 cp936（`encode_gb2312_dotnet`，E-4 口径）。
///
/// 牙：UTF-8 编码 ⇒ 下面的 cp936 字节序列与 UTF-8 排除断言都红。
#[test]
fn c_a22_6_sabuk_w_gbk_bytes() {
    let overlay = SabukWOverlay {
        castle_name: "沙巴克".to_string(),
        archers: std::array::from_fn(|_| SabukWUnitOverlay {
            name: "弓箭手".to_string(),
            ..SabukWUnitOverlay::default()
        }),
        guards: std::array::from_fn(|_| SabukWUnitOverlay {
            name: "守卫".to_string(),
            ..SabukWUnitOverlay::default()
        }),
        ..SabukWOverlay::default()
    };
    let out = serialize_sabuk_w(&IniOrdered::new(), &overlay, &SabukWDefaults::default())
        .expect("空 base 写回");

    // cp936：沙巴克 C9B3 B0CD BFCB；弓箭手 B9AD BCFD CAD6；守卫 CAD8 CEC0。
    let cp936: [&[u8]; 3] = [
        &[0xC9, 0xB3, 0xB0, 0xCD, 0xBF, 0xCB],
        &[0xB9, 0xAD, 0xBC, 0xFD, 0xCA, 0xD6],
        &[0xCA, 0xD8, 0xCE, 0xC0],
    ];
    for needle in cp936 {
        assert!(
            out.windows(needle.len()).any(|w| w == needle),
            "缺 cp936 字节序列 {needle:02X?}"
        );
    }
    // `StreamWriter(..., GB2312)` 无 BOM；任何 UTF-8 中文序列都不许出现。
    assert!(!out.starts_with(&[0xEF, 0xBB, 0xBF]));
    for needle in ["沙巴克".as_bytes(), "弓箭手".as_bytes(), "守卫".as_bytes()] {
        assert!(
            !out.windows(needle.len()).any(|w| w == needle),
            "输出里出现了 UTF-8 编码"
        );
    }
}

/// **C-A22-7（L）**：条件值语义（`CastleConf.cs:120-128 / 262-290`）。
///
/// - `total_gold=0` ⇒ 保留 base 的 `TotalGold=9`；`=5` ⇒ 覆盖成 5；
/// - `walls[2]=None`（RightWall 无实体）⇒ 保留 base 的 `RightWallHP=777`；
/// - `Archer_1` 无实体（`hp=0`）⇒ `Archer_1_HP=0`，但 `X/Y/Name` 仍写。
///
/// 牙：无条件写 0 ⇒ 第一条红；`Archer_i_HP` 写成配置 HP（2000）⇒ 最后一条红。
#[test]
fn c_a22_7_sabuk_w_conditional_values() {
    // base：显式 TotalGold=9 / RightWallHP=777 / Archer_1 配置（名字 cp936「弓箭手」）。
    let mut src = b"[Setup]\r\nTotalGold=9\r\n[Defense]\r\nRightWallHP=777\r\nArcher_1_X=11\r\nArcher_1_Y=22\r\nArcher_1_Name=".to_vec();
    src.extend_from_slice(&[0xB9, 0xAD, 0xBC, 0xFD, 0xCA, 0xD6]);
    src.extend_from_slice(b"\r\n");
    let base = parse_ordered(&src);

    let mut overlay = SabukWOverlay {
        archers: std::array::from_fn(|_| SabukWUnitOverlay::default()),
        ..SabukWOverlay::default()
    };
    overlay.total_gold = 0; // 不写 ⇒ 保留 base 的 9
    overlay.walls[2] = None; // RightWall 无实体 ⇒ 不写 HP
    overlay.archers[0] = SabukWUnitOverlay {
        name: "弓箭手".to_string(),
        x: 11,
        y: 22,
        hp: 0, // 无实体 ⇒ HP 写 0
    };

    let text = text_of(
        &serialize_sabuk_w(&base, &overlay, &SabukWDefaults::default()).expect("条件值写回"),
    );
    assert!(text.contains("TotalGold=9\r\n"), "0 不覆盖旧值");
    assert!(text.contains("RightWallHP=777\r\n"), "无实体不写 HP");
    assert!(text.contains("Archer_1_HP=0\r\n"), "无实体 HP 写 0");
    assert!(text.contains("Archer_1_X=11\r\n"));
    assert!(text.contains("Archer_1_Y=22\r\n"));
    assert!(text.contains("Archer_1_Name=弓箭手\r\n"));

    overlay.total_gold = 5;
    let text5 = text_of(
        &serialize_sabuk_w(&base, &overlay, &SabukWDefaults::default()).expect("gold=5 写回"),
    );
    assert!(text5.contains("TotalGold=5\r\n"), "非 0 覆盖");
    assert!(!text5.contains("TotalGold=9"));
}

/// `ConfigFile.Load` 的逐条语义（注释 / 大段注释 / `;;` 行内截断 / 重复键先到先得 /
/// 重复段停止 / 段头前的键落空名段）。
#[test]
fn parse_ordered_matches_config_file_load() {
    let src = b"; line comment\r\nkey_before=1\r\n/* large\r\nignored=2\r\n*/\r\n[Sec A]\r\nK=  v  \r\nK=second\r\nInline=abc;;tail\r\n[SEC A]\r\nGone=1\r\n";
    let parsed = parse_ordered(src);
    let sections: Vec<&str> = parsed.sections().iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(sections, ["", "Sec A"]);
    assert_eq!(parsed.get("", "key_before"), Some("1"));
    assert_eq!(
        parsed.get("Sec A", "K"),
        Some("v"),
        "值 Trim + 重复键先到先得"
    );
    assert_eq!(parsed.get("Sec A", "Inline"), Some("abc"), ";; 行内截断");
    assert_eq!(parsed.get("Sec A", "ignored"), None);
    assert_eq!(
        parsed.get("Sec A", "Gone"),
        None,
        "大小写不敏感的重复段 ⇒ 停止"
    );

    // `;;` 段注释态只被 `sec.Length >= 2` 的段头重置（`ConfigFile.cs:303-312`）：
    // `[;;skip]` 之后的单字符段 `[a]` 不重置 ⇒ 其后的键行仍被跳过。
    let parsed = parse_ordered(
        b"[;;skip]
[a]
K=v
",
    );
    assert!(parsed.contains_section("a"));
    assert_eq!(parsed.sections().len(), 1, "`[;;skip]` 不建段");
    assert_eq!(parsed.get("a", "K"), None, "单字符段名不重置 ;; 段注释态");

    // `[]` / `[  ]` 不是段头（`GetSecString` 返回空 ⇒ 当键值行处理）。
    let parsed = parse_ordered(b"[Setup]\r\n[]\r\n[  ]\r\nCastleName=x\r\n");
    assert!(parsed.contains_section("Setup"));
    assert_eq!(parsed.get("Setup", "CastleName"), Some("x"));
    assert_eq!(parsed.key_count(), 1);
}

fn temp_dir(tag: &str) -> PathBuf {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "a22p2-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

/// `write_sabuk_w`：`ConfigFile.Save` 的 `FileMode.Truncate` 语义——文件可新建，写后无残留尾巴。
#[test]
fn write_sabuk_w_truncates_and_creates() {
    let dir = temp_dir("sabukw");
    let path = dir.join("SabukW.txt");

    let bytes = deployed_bytes();
    let base = parse_ordered(&bytes);
    let overlay = SabukWOverlay::from_config(&parse_sabuk_w_bytes(&bytes));
    write_sabuk_w(&path, &base, &overlay, &SabukWDefaults::default()).expect("新建并写入");
    assert_eq!(std::fs::read(&path).expect("读回"), bytes);

    // 再写一份更短的最小输出 ⇒ 旧内容必须被截断（非原子原地截断，oracle 同）。
    let small = serialize_sabuk_w(
        &IniOrdered::new(),
        &SabukWOverlay::default(),
        &SabukWDefaults::default(),
    )
    .expect("最小输出");
    write_sabuk_w(
        &path,
        &IniOrdered::new(),
        &SabukWOverlay::default(),
        &SabukWDefaults::default(),
    )
    .expect("覆盖写");
    let on_disk = std::fs::read(&path).expect("读回");
    assert_eq!(on_disk, small);
    assert!(on_disk.len() < bytes.len());

    let _ = std::fs::remove_dir_all(&dir);
}

/// `write_attack_sabuk_wall`：文件不存在 ⇒ `EnvirError::Io` 且**不建文件**
/// （`StringList.SaveToFile` → `FileMode.Open` 抛 `FileNotFoundException`）；空表 ⇒ 0 字节。
#[test]
fn write_attack_sabuk_wall_requires_existing_file() {
    let dir = temp_dir("attack");
    let path = dir.join("AttackSabukWall.txt");
    let entries = [AttackSabukWallEntry {
        guild_name: "同盟会".to_string(),
        attack_date: "2026-10-15".to_string(),
        attack_date_parsed: true,
    }];

    let err = write_attack_sabuk_wall(&path, &entries).expect_err("缺文件必须报错");
    assert!(matches!(err, EnvirError::Io { .. }));
    assert!(!path.exists(), "oracle 不建文件");

    std::fs::write(&path, b"").expect("先建空文件（部署件 0 B）");
    write_attack_sabuk_wall(&path, &entries).expect("写申请");
    // 同盟会 = CDAC C3CB BBE1（cp936）+ 7 空格 + 引号日期 + CRLF。
    let mut expected = vec![0xCD, 0xAC, 0xC3, 0xCB, 0xBB, 0xE1];
    expected.extend_from_slice(b"       \"2026-10-15\"\r\n");
    assert_eq!(std::fs::read(&path).expect("读回"), expected);

    // 空表 ⇒ 0 字节（截断写，`StreamWriter(fileName, false, ...)`）。
    write_attack_sabuk_wall(&path, &[]).expect("空表");
    assert_eq!(std::fs::read(&path).expect("读回"), Vec::<u8>::new());

    let _ = std::fs::remove_dir_all(&dir);
}

/// `write_castle_list`：写下标 + CRLF；文件不存在 ⇒ 报错且不建文件。
#[test]
fn write_castle_list_requires_existing_file() {
    let dir = temp_dir("list");
    let path = dir.join("List.txt");

    assert!(write_castle_list(&path, 2).is_err());
    assert!(!path.exists());

    std::fs::write(&path, b"stale").expect("先建文件");
    write_castle_list(&path, 2).expect("写名单");
    assert_eq!(
        std::fs::read(&path).expect("读回"),
        &b"0\r\n1\r\n"[..],
        "SaveCastleList 写的是下标（CastleManager.cs:188）"
    );
    assert_eq!(
        std::fs::read(&path).expect("读回"),
        serialize_castle_list(2).into_bytes()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **F1（复核）**：`WriteDateTime` 归一——`DateTime.Parse` 后按 `yyyy-MM-dd HH:mm:ss` 写出。
///
/// 部署件 `0/SabukW.txt` 的 `ChangeDate=1998-01-01` / `WarDate=1998-01-01` 在 oracle 里
/// 回写成 `1998-01-01 00:00:00`（每键 +9 字节，两键共 +18）。
///
/// 牙：写侧保持原串（`normalize_sabuk_w_datetime` 退回 `raw`）⇒ 归一断言与
/// `!contains("ChangeDate=1998-01-01\r\n")` 都红。
#[test]
fn f1_sabuk_w_datetime_normalized() {
    let bytes = hex_bytes(DEPLOYED_ZERO_SABUK_W_HEX);
    let cfg = parse_sabuk_w_bytes(&bytes);
    // 读侧仍保留原始串（`DateTime.Parse` 之前）。
    assert_eq!(cfg.change_date.as_deref(), Some("1998-01-01"));
    assert_eq!(cfg.war_date.as_deref(), Some("1998-01-01"));
    assert_eq!(cfg.income_today.as_deref(), Some("2021-08-23 15:31:29"));

    let base = parse_ordered(&bytes);
    let overlay = SabukWOverlay::from_config(&cfg);
    let text = text_of(
        &serialize_sabuk_w(&base, &overlay, &SabukWDefaults::default()).expect("0/SabukW 写回"),
    );

    assert!(
        text.contains("ChangeDate=1998-01-01 00:00:00\r\n"),
        "`yyyy-MM-dd` 必须补成 `yyyy-MM-dd 00:00:00`"
    );
    assert!(text.contains("WarDate=1998-01-01 00:00:00\r\n"));
    assert!(text.contains("IncomeToday=2021-08-23 15:31:29\r\n"));
    assert!(
        !text.contains("ChangeDate=1998-01-01\r\n"),
        "原串必须被归一（否则每键少 9 字节）"
    );

    // `yyyy/M/d`、`T` 分隔、小数秒、两位字段：`DateTime.Parse` 兼容面。
    let overlay = SabukWOverlay {
        change_date: "1998/1/2".to_string(),
        war_date: "1998/01/02 3:4:5".to_string(),
        income_today: "2021-08-23T15:31:29.123".to_string(),
        ..SabukWOverlay::default()
    };
    let text = text_of(
        &serialize_sabuk_w(&IniOrdered::new(), &overlay, &SabukWDefaults::default())
            .expect("归一写回"),
    );
    assert!(text.contains("ChangeDate=1998-01-02 00:00:00\r\n"));
    assert!(text.contains("WarDate=1998-01-02 03:04:05\r\n"));
    assert!(text.contains("IncomeToday=2021-08-23 15:31:29\r\n"));
}

/// **Low-1（复核）**：六个配置面缺键默认值由调用方注入（oracle 取 `SystemShare.Config.*`，
/// `Server.conf` 可覆盖），不是编译期常量。
///
/// 牙：把 `write_effective_string/int` 与 `sabuk_w_write_back` 的默认值写回常量 ⇒ 前六条红。
#[test]
fn low1_sabuk_w_defaults_injected() {
    let defaults = SabukWDefaults {
        castle_name: "自定义城".to_string(),
        home_map: "5".to_string(),
        home_x: 111,
        home_y: 222,
        war_range_x: 333,
        war_range_y: 444,
    };
    let base = parse_ordered(b"[Setup]\r\nOwnGuild=\r\n");
    let text = text_of(
        &serialize_sabuk_w(&base, &SabukWOverlay::default(), &defaults).expect("注入默认写回"),
    );
    assert!(text.contains("CastleName=自定义城\r\n"));
    assert!(text.contains("CastleHomeMap=5\r\n"));
    assert!(text.contains("CastleHomeX=111\r\n"));
    assert!(text.contains("CastleHomeY=222\r\n"));
    assert!(text.contains("CastleWarRangeX=333\r\n"));
    assert!(text.contains("CastleWarRangeY=444\r\n"));

    // 读侧同一个 `LoadConfig` 缺键分支也要吃注入值。
    let cfg = SabukWConfig::from_ini_with_defaults(
        &mir2_data::exps::IniFile::from_bytes(b"[Setup]\r\n"),
        &defaults,
    );
    assert_eq!(cfg.castle_name, "自定义城");
    assert_eq!(cfg.home_map, "5");
    assert_eq!((cfg.home_x, cfg.home_y), (111, 222));
    assert_eq!((cfg.war_range_x, cfg.war_range_y), (333, 444));
}

/// **Low-2（复核）**：父目录不存在 ⇒ 建目录再写（`ConfigFile.Save` 的 `Directory.CreateDirectory`）。
///
/// 牙：去掉 `create_dir_all` ⇒ 写失败，`expect` 红。
#[test]
fn low2_write_sabuk_w_creates_parent_dirs() {
    let dir = temp_dir("mkdir");
    let path = dir.join("Castle").join("0").join("SabukW.txt");
    assert!(
        !path.parent().expect("有父目录").exists(),
        "前置：父目录不存在"
    );

    write_sabuk_w(
        &path,
        &IniOrdered::new(),
        &SabukWOverlay::default(),
        &SabukWDefaults::default(),
    )
    .expect("父目录不存在也要能写");
    assert!(path.exists());
    assert_eq!(
        std::fs::read(&path).expect("读回"),
        serialize_sabuk_w(
            &IniOrdered::new(),
            &SabukWOverlay::default(),
            &SabukWDefaults::default()
        )
        .expect("对照输出")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **Low-2（复核）**：只读文件 ⇒ 临时清 `IsReadOnly`，写成功后恢复。
///
/// 牙：去掉清位 ⇒ `std::fs::write` 在 Windows 上 PermissionDenied，`expect` 红；
/// 去掉恢复 ⇒ 最后一条只读断言红。
#[test]
fn low2_write_sabuk_w_toggles_readonly() {
    let dir = temp_dir("readonly");
    let path = dir.join("SabukW.txt");
    std::fs::write(&path, b"stale").expect("先建文件");
    let original_perms = std::fs::metadata(&path).expect("metadata").permissions();
    let mut perms = original_perms.clone();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms).expect("置只读");
    assert!(std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .readonly());

    write_sabuk_w(
        &path,
        &IniOrdered::new(),
        &SabukWOverlay::default(),
        &SabukWDefaults::default(),
    )
    .expect("只读文件要临时清位再写");
    assert!(
        std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .readonly(),
        "写后必须恢复只读"
    );
    assert_ne!(
        std::fs::read(&path).expect("读回"),
        b"stale".to_vec(),
        "内容要被覆盖"
    );

    std::fs::set_permissions(&path, original_perms).expect("恢复权限后清理");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **Low-3（复核）**：`MainDoorX/Y` 与三面墙 `*WallX/Y` 按 oracle 的 `ReadWriteInt16`
/// （`short`）解析：越界/非数字 ⇒ [`EnvirError::SabukWInt16Invalid`]，不再 i32 静默回落。
///
/// 牙：退回 `write_effective_int`（i32）⇒ `MainDoorX=40000` 被原样写出，`expect_err` 红。
#[test]
fn low3_main_door_and_wall_xy_reject_out_of_short() {
    let defaults = SabukWDefaults::default();

    let base = parse_ordered(b"[Defense]\r\nMainDoorX=40000\r\n");
    let err = serialize_sabuk_w(&base, &SabukWOverlay::default(), &defaults)
        .expect_err("short 越界必须报错");
    match err {
        EnvirError::SabukWInt16Invalid {
            section,
            key,
            value,
        } => assert_eq!(
            (section.as_str(), key.as_str(), value.as_str()),
            ("Defense", "MainDoorX", "40000")
        ),
        other => panic!("错误类型不对: {other:?}"),
    }

    // 非数字（oracle `Convert.ChangeType` 的 FormatException 分支）。
    for key in [
        "MainDoorY",
        "LeftWallX",
        "LeftWallY",
        "CenterWallX",
        "CenterWallY",
        "RightWallX",
        "RightWallY",
    ] {
        let src = format!("[Defense]\r\n{key}=abc\r\n");
        let base = parse_ordered(src.as_bytes());
        let err = serialize_sabuk_w(&base, &SabukWOverlay::default(), &defaults)
            .expect_err("非数字必须报错");
        assert!(
            matches!(err, EnvirError::SabukWInt16Invalid { .. }),
            "{key}: {err:?}"
        );
    }

    // i16 边界仍合法。
    let base = parse_ordered(b"[Defense]\r\nMainDoorX=-32768\r\nRightWallY=32767\r\n");
    let text = text_of(
        &serialize_sabuk_w(&base, &SabukWOverlay::default(), &defaults).expect("边界值合法"),
    );
    assert!(text.contains("MainDoorX=-32768\r\n"));
    assert!(text.contains("RightWallY=32767\r\n"));
}
