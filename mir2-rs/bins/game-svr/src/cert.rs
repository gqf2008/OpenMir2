//! 客户端认证链 —— `ChannelMessageHandler.cs` 的 `DoClientCertification`（`:291-360`）与
//! `GetCertification`（`:237-289`）。
//!
//! 口径（逐条对着行号）：
//! 1. `DoClientCertification` 拿到的 `sMsg` 是 **GM_DATA body 的 ASCII 渲染**（`:224` 的
//!    `msgBuff.ToString(Encoding.ASCII)`，即 `Encoding.ASCII.GetString`）；
//! 2. `HUtil32.TagCount(sMsg, '!') > 0` 才继续（`:309`）——否则**什么都不做**（既不认证也不关人）；
//! 3. `HUtil32.ArrestStringEx(sMsg, "#", "!", ref sMsg)`（`:311`）⇒ `sMsg` 变成 `"0<EDCode>"`；
//! 4. `GetCertification`：`sMsg[1..]` **跳过那个 `0`**（`:248`）→ `EDCode.DeCodeString` →
//!    要求前两位是 `**`（`:250`）→ 用 `/`（`HUtil32.Backslash`）切 7 段：
//!    `account / chrName / code / clientVersion / idx / hwid / gateId`（`:253-259`）；
//!    判定：`account`/`chrName`/`hwid` 非空、`StrToInt(code) >= 2`（`:269`）；
//!    `gateId == -1`（不可解析）⇒ 判否（`:275-279`）；`idx == "0"` ⇒ `boFlag = true`（`:261-268`）；
//! 5. 再过准入表：`GetAdmission(account, ip, sessionId)` 且 `nPayMent > 0`（`:314-315`），否则
//!    标 `*disable*` + `Certification=false` + `CloseUser`（`:340-353`）。

use mir2_shared::hutil32::{arrest_string_ex_ref, get_valid_str3, str_to_int, ArrestWrite};

use crate::auth::AdmissionStore;
use crate::hutil::tag_count;

/// 字段分隔符：`HUtil32.Backslash = '/'`（`HUtil32.cs:12`——名字叫 Backslash，值是斜杠）。
const FIELD_DIVIDER: [char; 1] = ['/'];

/// `GetCertification` 的产出（对应它的 ref 参数）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertParse {
    pub account: String,
    pub chr_name: String,
    /// `StrToInt(sCodeStr, 0)`（`:260`）；调用方要求 `>= 2`。
    pub session_id: i32,
    /// `sIdx == "0"`（`:261-268`）。
    pub flag: bool,
    /// `StrToInt(sClientVersion, 0)`。
    pub client_version: i32,
    /// `sHwid` 原文（oracle 在此做 `MD5.MD5UnPrInt`，S2b 取角色/落库时才需要）。
    pub hwid_raw: String,
    /// `StrToInt(sGateId, -1)`（`:275`）。
    pub gate_id: i32,
    /// 对应 C# 的返回值 `result`：字段**先赋值、后判有效**（`:253-279`）⇒ 失败时字段也可能是非空的
    /// （oracle 的失败日志 `会话验证失败.Account:{sAccount}` 正是这个原因）。
    pub valid: bool,
}

/// `GetCertification`（`:237-289`）。入参是第 3 步截出来的 `"0<EDCode>"`。
#[must_use]
pub fn get_certification(arrested: &str) -> CertParse {
    let empty = CertParse {
        account: String::new(),
        chr_name: String::new(),
        session_id: 0,
        flag: false,
        client_version: 0,
        hwid_raw: String::new(),
        gate_id: -1,
        valid: false,
    };
    // `sMsg[1..]`：跳过 `#` 后那个 `0`（`:248`）；空/单字符 ⇒ 后面必然失败
    let mut it = arrested.chars();
    if it.next().is_none() {
        return empty;
    }
    let encoded = it.as_str();
    // `EDCode.DeCodeString`：EDCode 解 + GBK 读（与 `ClientSession` 侧同一套语义）
    let bytes = mir2_protocol::edcode::decode(encoded.as_bytes());
    let text = crate::gbk_decode(&bytes);
    if text.chars().count() <= 2 || !text.starts_with("**") {
        return empty; // `sData.Length > 2` + 前两位 `**`（`:250`）——这一支不赋值任何 ref 字段
    }
    let body = &text[2..]; // 安全：前两个字符是 ASCII '*'
    let (account, r) = get_valid_str3(body, &FIELD_DIVIDER);
    let (chr_name, r) = get_valid_str3(&r, &FIELD_DIVIDER);
    let (code_str, r) = get_valid_str3(&r, &FIELD_DIVIDER);
    let (ver_str, r) = get_valid_str3(&r, &FIELD_DIVIDER);
    let (idx, r) = get_valid_str3(&r, &FIELD_DIVIDER);
    let (hwid, r) = get_valid_str3(&r, &FIELD_DIVIDER);
    let (gate_id_str, _) = get_valid_str3(&r, &FIELD_DIVIDER);

    let session_id = str_to_int(&code_str, 0);
    let flag = idx == "0";
    let gate_id = str_to_int(&gate_id_str, -1);
    let mut valid =
        !account.is_empty() && !chr_name.is_empty() && session_id >= 2 && !hwid.is_empty(); // `:269`
    if gate_id == -1 {
        valid = false; // `:275-279`：字段照样返回，只把 result 置否
    }
    CertParse {
        account,
        chr_name,
        session_id,
        flag,
        client_version: str_to_int(&ver_str, 0),
        hwid_raw: hwid,
        gate_id,
        valid,
    }
}

/// `DoClientCertification` 的判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertOutcome {
    /// 通过：调用方应置 `Certification=true` 并把 `LoadDBInfo`（取角色）交给世界侧（S2b）。
    Ok {
        cert: CertParse,
        /// 准入表里的会话（`GetAdmission` 的返回）。
        session_id: i32,
        pay_ment: i32,
    },
    /// 解析失败或准入不通过：照 oracle 标 `*disable*` + `Certification=false` + `CloseUser`；
    /// 带上能拿到的字段（oracle 的失败日志要打 `Account`/`SessionId`）。
    Failed {
        account: String,
        game_session_id: i32,
    },
    /// 连 `!` 都没有 ⇒ oracle 的 `if` 不成立，**什么都不做**（`:309`）。
    Ignored,
}

/// 认证链本体（`:305-355`）：ASCII 渲染 → `!` 计数 → 截 `#`…`!` → 解析 → 准入表。
#[must_use]
pub fn do_client_certification(body: &[u8], store: &AdmissionStore) -> CertOutcome {
    let s_msg = crate::hutil::ascii_from_bytes(body);
    if tag_count(&s_msg, '!') == 0 {
        return CertOutcome::Ignored;
    }
    // `:311` 是 `ArrestStringEx(sMsg, "#", "!", ref sMsg)`——**源与目标是同一个变量**：
    // 命中 ⇒ sMsg 被写成截出的内容；未命中（`Unchanged`：无闭合 `!` 等）⇒ sMsg **保持原文**
    // （第三态不能 collapse：`Run()` 里那个"空则中断"的判据就是靠 `sData` 是否被写过来区分的）。
    let (write, _rest) = arrest_string_ex_ref(&s_msg, '#', '!');
    let arrested = match write {
        ArrestWrite::Value(v) => v,
        ArrestWrite::Unchanged => s_msg,
    };
    let cert = get_certification(&arrested);
    if !cert.valid {
        return CertOutcome::Failed {
            account: cert.account,
            game_session_id: cert.session_id,
        };
    }
    let Some(admission) = store.get_admission(&cert.account, cert.session_id) else {
        return CertOutcome::Failed {
            account: cert.account,
            game_session_id: cert.session_id,
        };
    };
    if !admission.is_admitted() {
        return CertOutcome::Failed {
            account: cert.account,
            game_session_id: cert.session_id,
        };
    }
    CertOutcome::Ok {
        cert,
        session_id: admission.session.session_id,
        pay_ment: admission.pay_ment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AdmissionStore;

    /// 用**真 C# GameGate 实测产出的合成登录包**（98B，见
    /// `E:\tmp\openmir2-gamegate-imp\oracle-login-cap\out-csharp-raw.json`）当输入：
    /// 解出来必须是 `account=loadbot1 / chr=lb1a / code=2 / ver=120020522 / idx=1200205220 / hwid=30 个 0 / gateId=0`。
    const ORACLE_LOGIN_BODY: &str = "23305e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b5f603f21";

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// 解析层：oracle 实测包必须解析出预期字段（这是"我解的就是 oracle 解的那条"的直接判据）。
    #[test]
    fn parses_oracle_login_body() {
        let body = hex(ORACLE_LOGIN_BODY);
        let s_msg = crate::hutil::ascii_from_bytes(&body);
        assert_eq!(tag_count(&s_msg, '!'), 1);
        let (write, _rest) = arrest_string_ex_ref(&s_msg, '#', '!');
        let arrested = match write {
            ArrestWrite::Value(v) => v,
            ArrestWrite::Unchanged => s_msg.clone(),
        };
        let cert = get_certification(&arrested);
        assert!(cert.valid, "oracle 实测包必须解析通过");
        assert_eq!(cert.account, "loadbot1");
        assert_eq!(cert.chr_name, "lb1a");
        assert_eq!(cert.session_id, 2, "code 段（第 3 段）= \"2\"");
        assert_eq!(cert.client_version, 120020522);
        assert!(!cert.flag, "idx 段（第 5 段）= 1200205220 ≠ \"0\"");
        assert_eq!(cert.hwid_raw.len(), 30, "hwid = 30 个 0");
        assert_eq!(cert.gate_id, 0);
    }

    /// 准入表通过 ⇒ 认证成功（`DoClientCertification` 的**正向路径**，用真包 + 预置会话）。
    #[test]
    fn oracle_packet_with_admission_certifies() {
        let mut store = AdmissionStore::new();
        store.new_session("loadbot1", "127.0.0.1", 2, 0, 1, 3600);
        match do_client_certification(&hex(ORACLE_LOGIN_BODY), &store) {
            CertOutcome::Ok {
                cert,
                session_id,
                pay_ment,
            } => {
                assert_eq!(cert.account, "loadbot1");
                assert_eq!(cert.chr_name, "lb1a");
                assert_eq!(session_id, 2);
                assert_eq!(pay_ment, 1, "PayMent=0 ⇒ nPayMent=1");
            }
            other => panic!("应认证通过，实得 {other:?}"),
        }
    }

    /// 没有准入记录 / 准入被停（PayMent=3 映射成 0）⇒ Failed（oracle `:340-346` 的"会话验证失败"）。
    #[test]
    fn missing_or_disabled_admission_fails() {
        let empty = AdmissionStore::new();
        match do_client_certification(&hex(ORACLE_LOGIN_BODY), &empty) {
            CertOutcome::Failed {
                account,
                game_session_id,
            } => {
                assert_eq!(
                    account, "loadbot1",
                    "失败日志里的 Account 也要有（oracle 先赋值后判）"
                );
                assert_eq!(game_session_id, 2);
            }
            other => panic!("应失败，实得 {other:?}"),
        }
        let mut disabled = AdmissionStore::new();
        disabled.new_session("loadbot1", "127.0.0.1", 2, 3, 0, 0); // PayMent=3 ⇒ nPayMent=0
        assert!(matches!(
            do_client_certification(&hex(ORACLE_LOGIN_BODY), &disabled),
            CertOutcome::Failed { .. }
        ));
    }

    /// 没有 `!` ⇒ oracle 的 `if` 不成立 ⇒ **什么都不做**（既不认证也不关人）。
    #[test]
    fn frame_without_bang_is_ignored() {
        let store = AdmissionStore::new();
        assert_eq!(
            do_client_certification(b"#0nothing", &store),
            CertOutcome::Ignored
        );
    }

    /// 解析层的负例：`**` 缺失、字段不足、`gateId` 不可解析 ⇒ `valid=false`（字段仍尽量填）。
    #[test]
    fn parse_negatives() {
        assert!(!get_certification("").valid);
        assert!(!get_certification("0").valid, "跳掉 0 后没有内容");
        // 前两位不是 `**`
        let enc = mir2_protocol::edcode::encode(b"XXa/b/2/1/0/h/0");
        let mut s = String::from("0");
        s.push_str(&String::from_utf8(enc).unwrap());
        assert!(!get_certification(&s).valid, "前两位不是 ** ⇒ 判否");
        // 字段不足（没有 gateId 段）⇒ 第 7 段取空 ⇒ StrToInt 取 -1 ⇒ 判否
        let enc2 = mir2_protocol::edcode::encode(b"**a/b/2/1/0/h");
        let mut s2 = String::from("0");
        s2.push_str(&String::from_utf8(enc2).unwrap());
        let p2 = get_certification(&s2);
        assert!(!p2.valid, "缺 gateId ⇒ 判否");
        assert_eq!(p2.account, "a", "字段照样被赋值（照 oracle 的 ref 语义）");
        assert_eq!(p2.gate_id, -1);
    }
}
