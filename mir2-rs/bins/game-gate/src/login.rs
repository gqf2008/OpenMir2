//! 客户端 face 的**登录帧**解析（切片 2）——照 `src/GameGate/Services/ClientSession.cs:1058-`。
//!
//! 为什么必须有这一步：客户端连上 7200 后发的第一帧**不是**普通命令帧，而是 `ClientLogin`
//! （`ClientSession.cs:655-680` 的分派：内层不是 `CommandMessage` 时走 `:671-680`）。
//! 网关校验它，**失败即剔除**（`Kick(1)`，日志「客户端登陆消息处理失败，剔除链接」）；
//! 只有通过后才把 `HandleLogin` 置真（`:1228`），后续帧才按命令处理（`:153`）。
//! 未通过登录就把帧转给 GameSvr，是 **比 oracle 宽的偏差**（协议梳理 §8 第 1 条）。

/// 登录帧的最大长度（`ClientLogin` 里的 `firstPakcetMaxLen`，`ClientSession.cs:1060`）。
const FIRST_PACKET_MAX_LEN: usize = 254;

/// C# `HUtil32.GetValidStr3(string source, ref string dest, char divider)`（`HUtil32.cs:384-389`）：
/// `source.Split(divider, 2, RemoveEmptyEntries)` ⇒ `dest = ary[0]`、返回 `ary[1]`（不足补空串）。
///
/// 注意两处容易照错的语义：① **只切一次**（`count = 2`）；② `RemoveEmptyEntries`
/// ⇒ 以分隔符开头的串会让 `ary[0]` 丢掉，`dest` 变成剩余部分。
#[must_use]
pub fn get_valid_str3(source: &str, divider: char) -> (String, String) {
    let mut parts: Vec<&str> = Vec::with_capacity(2);
    match source.split_once(divider) {
        Some((head, tail)) => {
            parts.push(head);
            parts.push(tail);
        }
        None => parts.push(source),
    }
    parts.retain(|p| !p.is_empty());
    let dest = parts.first().copied().unwrap_or("").to_string();
    let rest = parts.get(1).copied().unwrap_or("").to_string();
    (dest, rest)
}

/// 解析并校验登录文本；`Ok(())` = 通过（调用方据此把该会话标记为已登录）。
///
/// 覆盖 `ClientLogin` 的 ①②③④ 步（`ClientSession.cs:1060-1101`）：
/// - `packet_len`（外层 `PackLength`）必须落在 `(15, 254)`；
/// - 文本前两位必须是 `**`；
/// - 反斜杠切 `account \ charName \ cert \ clientVerNo \ code \ hardwareId`；
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
pub fn parse_client_login(text: &str, packet_len: usize) -> Result<(), String> {
    if packet_len >= FIRST_PACKET_MAX_LEN || packet_len <= 15 {
        return Err(format!("packetLen={packet_len} 不在 (15,254)"));
    }
    if !text.starts_with("**") {
        // 取前 8 个**字符**：别按字节切——实测日志里会出现中文/乱码前缀，按字节切会 panic
        let head: String = text.chars().take(8).collect();
        return Err(format!("Kicked 1: 前两位不是 **（实得 {head:?}）"));
    }
    let mut rest = text[2..].to_string();
    let (account, r) = get_valid_str3(&rest, '\\');
    rest = r;
    let (char_name, r) = get_valid_str3(&rest, '\\');
    rest = r;
    let n_account = account.chars().count();
    let n_char = char_name.chars().count();
    if !(4..=12).contains(&n_account) || !(3..=14).contains(&n_char) {
        return Err(format!(
            "账号/角色名长度不符：account={n_account} char={n_char}"
        ));
    }
    let (cert, r) = get_valid_str3(&rest, '\\');
    rest = r;
    let (client_ver, r) = get_valid_str3(&rest, '\\');
    rest = r;
    let (code, _hwid) = get_valid_str3(&rest, '\\');
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按 oracle 的长度规则造一条**应通过**的登录串（网关日志里现网账号/角色名形态：
    /// `loadbot1`/`lb1a`/9 位版本号/10 位 code）。
    #[test]
    fn deployed_shaped_login_passes() {
        let text = "**loadbot1\\lb1a\\1\\120020522\\1200205220\\AABB";
        assert!(
            parse_client_login(text, 60).is_ok(),
            "现网形态应通过：{text}"
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
            parse_client_login("**abc\\lb1a\\1\\120020522\\1200205220\\AA", 60).is_err(),
            "账号 3 位应失败"
        );
        assert!(
            parse_client_login("**loadbot1\\ab\\1\\120020522\\1200205220\\AA", 60).is_err(),
            "角色名 2 位应失败"
        );
        assert!(
            parse_client_login("**loadbot1\\lb1a\\1\\12002\\1200205220\\AA", 60)
                .unwrap_err()
                .contains("Kicked 2"),
            "版本号 <8 应报 Kicked 2"
        );
        assert!(
            parse_client_login("**loadbot1\\lb1a\\1\\120020522\\12002052\\AA", 60).is_err(),
            "code 非 10 位应失败"
        );
        assert!(
            parse_client_login("**loadbot1\\lb1a\\\\120020522\\1200205220\\AA", 60).is_err(),
            "cert 空应失败"
        );
        assert!(
            parse_client_login("**loadbot1\\lb1a\\1\\120020522\\1200205220\\AA", 300).is_err(),
            "packetLen≥254 应失败"
        );
    }

    /// `GetValidStr3` 的两个易错语义：只切一次 + RemoveEmptyEntries。
    #[test]
    fn get_valid_str3_semantics() {
        assert_eq!(
            get_valid_str3("a\\b\\c", '\\'),
            ("a".to_string(), "b\\c".to_string()),
            "只切一次，其余留给下一次"
        );
        assert_eq!(
            get_valid_str3("\\tail", '\\'),
            ("tail".to_string(), String::new()),
            "RemoveEmptyEntries：以分隔符开头时 dest 变成剩余部分"
        );
        assert_eq!(
            get_valid_str3("nosep", '\\'),
            ("nosep".to_string(), String::new())
        );
    }
}
