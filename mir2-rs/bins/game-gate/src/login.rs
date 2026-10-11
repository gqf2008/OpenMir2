//! 客户端 face 的**登录帧**解析（切片 2）——照 `src/GameGate/Services/ClientSession.cs:1058-`。
//!
//! 为什么必须有这一步：客户端连上 7200 后发的第一帧**不是**普通命令帧，而是 `ClientLogin`
//! （`ClientSession.cs:655-680` 的分派：内层不是 `CommandMessage` 时走 `:671-680`）。
//! 网关校验它，**失败即剔除**（`Kick(1)`，日志「客户端登陆消息处理失败，剔除链接」）；
//! 只有通过后才把 `HandleLogin` 置真（`:1228`），后续帧才按命令处理（`:153`）。
//! 未通过登录就把帧转给 GameSvr，是 **比 oracle 宽的偏差**（协议梳理 §8 第 1 条）。

/// 登录帧的最大长度（`ClientLogin` 里的 `firstPakcetMaxLen`，`ClientSession.cs:1060`）。
const FIRST_PACKET_MAX_LEN: usize = 254;

/// 登录文本的字段分隔符。**照 oracle 的事实值，不照名字**：`src/OpenMir2/HUtil32.cs:12` 是
/// `public const char Backslash = '/';` —— 名字叫 Backslash，值是**斜杠**；`ClientLogin` 用
/// `HUtil32.GetValidStr3(..., HUtil32.Backslash)` 切 `account/charName/cert/clientVerNo/code/hardwareId`。
///
/// 实机实测（真 C# 网关 + 真 Delphi 客户端的金标准帧）：`**loadbot1/lb1a/2/120020522/1200205220/46F78C…`
/// 被切成 account=`loadbot1`(8)/char=`lb1a`(4)/cert=`2`/ver=`120020522`(9)/code=`1200205220`(10) 并**通过**登录；
/// 若按 `'\\'` 切，account 会等于整段（长度 >12）⇒ 必然被剔除。证据：
/// `E:\tmp\openmir2-gamegate-imp\oracle-login-cap\FINDINGS.md`。
pub const FIELD_DIVIDER: char = '/';

/// C# `HUtil32.GetValidStr3(string source, ref string dest, char divider)`（`HUtil32.cs:384-389`）：
/// `source.Split(divider, 2, RemoveEmptyEntries)` ⇒ `dest = ary[0]`、返回 `ary[1]`（不足补空串）。
///
/// 逐字语义（**实机实测**得出，不是按文档直觉推断）：`count = 2` + `RemoveEmptyEntries` 合起来等于
/// 「**连续分隔符并作一个、首尾分隔符吃掉**」：
///
/// 1. 先跳过**开头**的所有分隔符；
/// 2. `dest` = 从那里起的第一个非空片段（整串都是分隔符 ⇒ `dest` 为空）；
/// 3. 再跳过**紧随其后**的所有分隔符，**其余原样**（含其中的分隔符）作为返回值。
///
/// 证据：`E:\tmp\openmir2-watch\splittest\`——真 .NET 9 跑 `String.Split(char,2,RemoveEmptyEntries)`
/// 与本实现逐例对比（`split-truth-table.txt`，21 例全 OK）。**旧实现**（`split_once` + `retain`）
/// 只在「无连续/前导分隔符」时相等，其余形态会把首段吞并：例如 `\acc\name\cert\ver\code`
/// C# 给 (`acc`,`name\cert\ver\code`)、旧实现给 (`acc\name\cert\ver\code`,``)⇒ 字段错位 ⇒ **假踢**。
/// 这正是 reviewer-1 在 `openmir2-m2-gamegate` 上的 Important #2。
#[must_use]
pub fn get_valid_str3(source: &str, divider: char) -> (String, String) {
    let head = source.trim_start_matches(divider);
    if head.is_empty() {
        return (String::new(), String::new());
    }
    match head.split_once(divider) {
        None => (head.to_string(), String::new()),
        Some((dest, tail)) => (
            dest.to_string(),
            tail.trim_start_matches(divider).to_string(),
        ),
    }
}

/// `ClientLogin` 校验通过后的字段（`ClientSession.cs:1077-1085` 的解析顺序与变量名）——
/// 合成登录包（`synthetic_login_body`）要用它们，所以校验函数必须把值带出来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientLoginFields {
    /// `sAccount`（账号）。
    pub account: String,
    /// `sHumName`（角色名）。
    pub char_name: String,
    /// `szCert`。
    pub cert: String,
    /// `szClientVerNo`（客户端版本号）。
    pub client_ver: String,
    /// `szCode`。
    pub code: String,
}

/// 登录通过后**必须**发给 GameSvr 的合成登录包（`ClientSession.cs:1199-1224`）。
///
/// 这是服务端认识该会话的唯一凭据：漏发则 GameSvr 永远等不到登录（reviewer-1 在
/// `openmir2-m2-gamegate` 上的 Important #1）。逐字复刻：
///
/// - 文本 = `**{account}/{charName}/{cert}/{clientVer}/{code}/{hardware}/{ServiceId}`
///   —— 注意分隔符是 `/`，与客户端 face 的 `\` **不同**（`:1203` 的插值串）；
/// - `hardware`：现网 `E:\MirServer\RunGate\config.conf` 的 `[Switch] ProcClientCount=0`
///   ⇒ `Config.IsProcClientHardwareID=false` ⇒ **30 个 `0`**（`:1202` 三元表达式的 false 分支；
///   字面量就是 `"000000000000000000000000000000"`，**数一下再写**：我先前凭目测写成 32，
///   是 oracle 字节回归用例（`oracle_golden_login_frame_yields_oracle_synthetic_packet`）把它抓出来的）；
/// - `ServiceId`：该端点线号（`Conf\ConfigManager.cs:18`：`GateList[i].ServiceId = (byte)i`）；
/// - body = `#0` + `EncryptUtil.Encode(GBK(文本))` + `!`，`PackLength = encodeLen + 3`（`:1204-1218`）。
#[must_use]
pub fn synthetic_login_body(f: &ClientLoginFields, service_id: u8) -> Vec<u8> {
    /// `ClientSession.cs:1202` 里 `IsProcClientHardwareID == false` 时的常量。
    const HARDWARE_DISABLED: &str = "000000000000000000000000000000";
    let text = format!(
        "**{}/{}/{}/{}/{}/{HARDWARE_DISABLED}/{service_id}",
        f.account, f.char_name, f.cert, f.client_ver, f.code
    );
    let encoded = mir2_protocol::edcode::encode(&crate::gbk::encode(&text));
    let mut body = Vec::with_capacity(encoded.len() + 3);
    body.extend_from_slice(b"#0");
    body.extend_from_slice(&encoded);
    body.push(b'!');
    body
}

/// 解析并校验登录文本；`Ok(())` = 通过（调用方据此把该会话标记为已登录）。
///
/// 覆盖 `ClientLogin` 的 ①②③④ 步（`ClientSession.cs:1060-1101`）：
/// - `packet_len`（外层 `PackLength`）必须落在 `(15, 254)`；
/// - 文本前两位必须是 `**`；
/// - 用 `FIELD_DIVIDER`（= `/`，见该常量的注释）切
///   `account / charName / cert / clientVerNo / code / hardwareId`；
/// - `4 ≤ len(account) ≤ 12`、`2 < len(charName) < 15`；
/// - `0 < len(cert) ≤ 8`、`len(clientVerNo) ≥ 8`、`len(code) == 10`。
///
/// **不在本切片**（属切片 3，登记不静默跳过）：`IsProcClientHardwareID` 的硬件 ID 变换、
/// 以及 `Kicked 4..7` 的账号/角色核对与封禁名单。
///
/// 长度口径：C# 用 `string.Length`（UTF-16 码元数）；这里用 `chars().count()`（BMP 内等价，
/// 代理对不在现网账号/角色名语料里），**不用字节数**——GBK 汉字是 2 字节，用字节数会把长度判宽一倍。
///
/// # Errors
/// 返回人类可读的失败原因（照 oracle 的 `Kicked N` 措辞），调用方据此关会话。
pub fn parse_client_login(text: &str, packet_len: usize) -> Result<ClientLoginFields, String> {
    if packet_len >= FIRST_PACKET_MAX_LEN || packet_len <= 15 {
        return Err(format!("packetLen={packet_len} 不在 (15,254)"));
    }
    if !text.starts_with("**") {
        // 取前 8 个**字符**：别按字节切——实测日志里会出现中文/乱码前缀，按字节切会 panic
        let head: String = text.chars().take(8).collect();
        return Err(format!("Kicked 1: 前两位不是 **（实得 {head:?}）"));
    }
    let mut rest = text[2..].to_string();
    let (account, r) = get_valid_str3(&rest, FIELD_DIVIDER);
    rest = r;
    let (char_name, r) = get_valid_str3(&rest, FIELD_DIVIDER);
    rest = r;
    let n_account = account.chars().count();
    let n_char = char_name.chars().count();
    if !(4..=12).contains(&n_account) || !(3..=14).contains(&n_char) {
        return Err(format!(
            "账号/角色名长度不符：account={n_account} char={n_char}"
        ));
    }
    let (cert, r) = get_valid_str3(&rest, FIELD_DIVIDER);
    rest = r;
    let (client_ver, r) = get_valid_str3(&rest, FIELD_DIVIDER);
    rest = r;
    let (code, _hwid) = get_valid_str3(&rest, FIELD_DIVIDER);
    let n_cert = cert.chars().count();
    if n_cert == 0 || n_cert > 8 {
        return Err(format!("cert 长度不符（{n_cert}）"));
    }
    if client_ver.chars().count() < 8 {
        return Err(format!("Kicked 2: clientVer 校验失败（{client_ver}）"));
    }
    if code.chars().count() != 10 {
        return Err(format!("code 长度必须 10（实得 {}）", code.chars().count()));
    }
    Ok(ClientLoginFields {
        account,
        char_name,
        cert,
        client_ver,
        code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按 oracle 的长度规则造一条**应通过**的登录串（现网账号/角色名形态：
    /// `loadbot1`/`lb1a`/9 位版本号/10 位 code）。分隔符必须是 `/`（见 `FIELD_DIVIDER`）。
    #[test]
    fn deployed_shaped_login_passes() {
        let text = "**loadbot1/lb1a/1/120020522/1200205220/AABB";
        assert!(
            parse_client_login(text, 60).is_ok(),
            "现网形态应通过：{text}"
        );
    }

    /// **金标准里那条真客户端登录文本**必须通过、且字段切对。出处：
    /// `tests/golden/session-20261010-c3-hook/capture.jsonl` seq 13（真 Delphi 客户端发的 7200 第一帧，
    /// 111 字节）→ EDCode 解出 `**loadbot1/lb1a/2/120020522/1200205220/46F78C6C…`；真 C# 网关实测**接受**它
    /// 并据此发出合成登录包（`E:\tmp\openmir2-gamegate-imp\oracle-login-cap\FINDINGS.md`）。
    /// 这条用例的作用：**谁把分隔符改回 `\`，它就变红**（那是"真客户端全被剔除"的形态）。
    #[test]
    fn golden_client_login_text_parses_with_deployed_divider() {
        let text =
            "**loadbot1/lb1a/2/120020522/1200205220/46F78C6CF966659119B8DC81EF975AD168F6E79C53";
        let f = parse_client_login(text, 111).expect("oracle 实测接受这条，Rust 必须也接受");
        assert_eq!(f.account, "loadbot1");
        assert_eq!(f.char_name, "lb1a");
        assert_eq!(f.cert, "2");
        assert_eq!(f.client_ver, "120020522");
        assert_eq!(f.code, "1200205220");
        // 反证：同一个文本若按 `\` 切（旧实现的假设）必然失败 —— 这就是"全被剔除"的机理
        assert!(
            parse_client_login("**loadbot1\\lb1a\\2\\120020522\\1200205220\\46F78C", 111).is_err(),
            "用 `\\` 形态的串应当失败（account 会等于整段、长度超限）"
        );
    }

    /// 前两位不是 `**` ⇒ Kicked 1（本次实机取证里真网关就是这条）。
    #[test]
    fn missing_star_prefix_is_kicked_1() {
        let e = parse_client_login("?h讣h讣**loadbot1\\lb1a\\1\\120020522\\1200205220\\AA", 60)
            .unwrap_err();
        assert!(e.contains("Kicked 1"), "应报 Kicked 1：{e}");
    }

    /// 账号太短 / 版本号太短 / code 长度不对 ⇒ 各自失败（对应 ClientLogin 的 ③④ 步）。
    #[test]
    fn length_rules_match_oracle() {
        assert!(
            parse_client_login("**abc/lb1a/1/120020522/1200205220/AA", 60).is_err(),
            "账号 3 位应失败"
        );
        assert!(
            parse_client_login("**loadbot1/ab/1/120020522/1200205220/AA", 60).is_err(),
            "角色名 2 位应失败"
        );
        assert!(
            parse_client_login("**loadbot1/lb1a/1/12002/1200205220/AA", 60)
                .unwrap_err()
                .contains("Kicked 2"),
            "版本号 <8 应报 Kicked 2"
        );
        assert!(
            parse_client_login("**loadbot1/lb1a/1/120020522/12002052/AA", 60).is_err(),
            "code 非 10 位应失败"
        );
        assert!(
            parse_client_login("**loadbot1/lb1a//120020522/1200205220/AA", 60).is_err(),
            "cert 空应失败"
        );
        assert!(
            parse_client_login("**loadbot1/lb1a/1/120020522/1200205220/AA", 300).is_err(),
            "packetLen≥254 应失败"
        );
    }

    /// `GetValidStr3` 的逐字语义，**逐例对齐真 .NET 实测表**（`E:\tmp\openmir2-watch\splittest\
    /// split-truth-table.txt`，21 例）。这里把整张表钉进单测：连续分隔符并作一个、首尾分隔符被吃掉。
    #[test]
    fn get_valid_str3_semantics() {
        let cases: &[(&str, &str, &str)] = &[
            (r"a\b\c", "a", r"b\c"),
            (r"\tail", "tail", ""),
            ("nosep", "nosep", ""),
            (r"a\\b", "a", "b"),
            (r"\\a", "a", ""),
            (r"\\\a", "a", ""),
            (r"\a\b", "a", "b"),
            (r"a\", "a", ""),
            (r"\", "", ""),
            ("", "", ""),
            (r"acc\name\\ver\code\hwid", "acc", r"name\\ver\code\hwid"),
            (r"acc\name\cert\\code\hwid", "acc", r"name\cert\\code\hwid"),
            // reviewer-1 的 Important #2 用例：旧实现会把它整段当首段 ⇒ 字段错位 ⇒ 假踢
            (r"\acc\name\cert\ver\code", "acc", r"name\cert\ver\code"),
            (
                r"acc\\name\cert\ver\code\hwid",
                "acc",
                r"name\cert\ver\code\hwid",
            ),
            (
                r"acc\name\\\cert\ver\code\hwid",
                "acc",
                r"name\\\cert\ver\code\hwid",
            ),
            (r"a\\\b\c", "a", r"b\c"),
            (r"a\\", "a", ""),
            (r"\\a\\", "a", ""),
            (r"a\b\\c", "a", r"b\\c"),
            (r"\\\\", "", ""),
            (
                r"acc\name\cert\ver\code\hwid",
                "acc",
                r"name\cert\ver\code\hwid",
            ),
        ];
        for (src, dest, rest) in cases {
            assert_eq!(
                get_valid_str3(src, '\\'),
                ((*dest).to_string(), (*rest).to_string()),
                "src={src:?}（口径见 split-truth-table.txt）"
            );
        }
    }

    /// 前导分隔符不能把账号吃掉：`/loadbot1/lb1a/...` 在 C# 里仍然解析出账号 `loadbot1`
    /// （旧实现会给 `loadbot1/lb1a/...` ⇒ 账号长度 >12 ⇒ 假踢）。
    #[test]
    fn leading_separator_login_is_not_falsely_kicked() {
        // 外层 `**` 之后紧跟一个多余的分隔符（truth table 第 13 行的同形）
        let text = "**/loadbot1/lb1a/1/120020522/1200205220/AABB";
        let f = parse_client_login(text, 60).expect("C# 接受这串，Rust 也必须接受");
        assert_eq!(f.account, "loadbot1");
        assert_eq!(f.char_name, "lb1a");
    }

    /// 合成登录包的**文本公式**独立复核：把产出的 body 解回明文再比
    /// （不是用同一个 `format!` 自证），并钉住 `#0…!` 外形与 `PackLength = encodeLen + 3`。
    #[test]
    fn synthetic_login_body_matches_oracle_formula() {
        let f = ClientLoginFields {
            account: "loadbot1".to_string(),
            char_name: "lb1a".to_string(),
            cert: "1".to_string(),
            client_ver: "120020522".to_string(),
            code: "1200205220".to_string(),
        };
        for service_id in [0u8, 2] {
            let body = synthetic_login_body(&f, service_id);
            assert_eq!(&body[..2], b"#0", "合成登录包以 `#0` 开头（:1207-1208）");
            assert_eq!(*body.last().unwrap(), b'!', "尾是 `!`（:1209）");
            let text = crate::gbk::decode(&mir2_protocol::edcode::decode(&body[2..body.len() - 1]));
            assert_eq!(
                text,
                format!(
                    "**loadbot1/lb1a/1/120020522/1200205220/\
                     000000000000000000000000000000/{service_id}"
                ),
                "文本必须逐字等于 :1203 的插值串（含 30 个 0 的 hardware 与末尾 ServiceId）"
            );
            let encode_len = mir2_protocol::edcode::encode(&crate::gbk::encode(&text)).len();
            assert_eq!(
                body.len(),
                encode_len + 3,
                "PackLength = encodeLen + 3（:1218）"
            );
        }
    }
}
