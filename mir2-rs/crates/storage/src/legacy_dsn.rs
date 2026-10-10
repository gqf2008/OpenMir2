//! 遗留 MySQL 连接串（MySQL Connector 风格）解析 → `sqlx` URL。
//!
//! 参照：`dbsvr.conf` / `logsrv.conf` 的 `[DataBase] ConnctionString`，形如
//! `server=127.0.0.1;uid=root;pwd=;database=mir2_db;`（键名与原拼写照抄，含 `pwd`）。
//!
//! 行为约定（**配置一字不改**，只做读取侧翻译）：
//! - 键名大小写不敏感；值为空串是合法的（`pwd=` 即空口令，现网就是空口令）；
//! - 已知键：`server`/`host`、`port`、`uid`/`user`、`pwd`/`password`、`database`/`db`；
//! - 未知键**忽略**（遗留文件里有 `SslMode` 一类，C# 侧同样不关心）；
//! - 缺 `server` 或 `database` ⇒ 报错（M1 验收④"缺配置必须明确报错退出"）。

use std::fmt::Write as _;

use crate::StorageError;

/// 解析后的连接参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDsn {
    /// 主机（`server`/`host`）。
    pub host: String,
    /// 端口（`port`，缺省 3306）。
    pub port: u16,
    /// 用户名（`uid`/`user`）。
    pub user: String,
    /// 口令（`pwd`/`password`，可为空串 —— 现网即空口令）。
    pub password: String,
    /// 库名（`database`/`db`）。
    pub database: String,
}

impl LegacyDsn {
    /// 解析 `k=v;k=v;` 形式；缺关键键或端口非法即报错。
    ///
    /// # Errors
    /// 缺 `server`/`database`、端口非法时返回 [`StorageError::Backend`]。
    pub fn parse(s: &str) -> Result<Self, StorageError> {
        let mut host = None;
        let mut port = None;
        let mut user = None;
        let mut password = None;
        let mut database = None;
        for segment in s.split(';') {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            let Some((k, v)) = segment.split_once('=') else {
                continue; // 无等号的残片：C# 侧也不会用到
            };
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim().to_string();
            match k.as_str() {
                "server" | "host" | "data source" | "datasource" => host = Some(v),
                "port" => port = Some(v),
                "uid" | "user" | "user id" | "userid" => user = Some(v),
                "pwd" | "password" => password = Some(v),
                "database" | "db" | "initial catalog" => database = Some(v),
                _ => {} // 未知键忽略
            }
        }
        let host =
            host.ok_or_else(|| StorageError::Backend(format!("连接串缺少 server：{s:?}")))?;
        let database =
            database.ok_or_else(|| StorageError::Backend(format!("连接串缺少 database：{s:?}")))?;
        let port = match port {
            None => 3306,
            Some(p) => p
                .parse::<u16>()
                .map_err(|_| StorageError::Backend(format!("连接串 port 非法：{p:?}")))?,
        };
        Ok(Self {
            host,
            port,
            user: user.unwrap_or_default(),
            password: password.unwrap_or_default(),
            database,
        })
    }

    /// 转 `sqlx` 的 MySQL URL（口令按 URL 规则转义）。
    #[must_use]
    pub fn to_sqlx_url(&self) -> String {
        let mut out = String::with_capacity(64);
        let _ = write!(out, "mysql://{}", url_encode(&self.user));
        if !self.password.is_empty() {
            let _ = write!(out, ":{}", url_encode(&self.password));
        }
        let _ = write!(out, "@{}:{}/{}", self.host, self.port, self.database);
        out
    }
}

/// URL 用户信息段的转义（`@`/`:`/`/`/`?`/`#`/`%` 等）。
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 现网两份配置的真实取值（`dbsvr.conf` / `logsrv.conf`）。
    #[test]
    fn parses_deployed_values() {
        let db = LegacyDsn::parse("server=127.0.0.1;uid=root;pwd=;database=mir2_db;").unwrap();
        assert_eq!(
            db,
            LegacyDsn {
                host: "127.0.0.1".into(),
                port: 3306,
                user: "root".into(),
                password: String::new(),
                database: "mir2_db".into(),
            }
        );
        assert_eq!(db.to_sqlx_url(), "mysql://root@127.0.0.1:3306/mir2_db");

        let acct =
            LegacyDsn::parse("server=127.0.0.1;uid=root;pwd=;database=mir2_account;").unwrap();
        assert_eq!(acct.database, "mir2_account");
    }

    #[test]
    fn tolerates_case_spacing_unknowns_and_ports() {
        let d = LegacyDsn::parse(
            "Server = 10.0.0.9 ; UID=sa ; PWD=p@ss:w/rd ; Database=Mir2 ; SslMode=None;",
        )
        .unwrap();
        assert_eq!(d.host, "10.0.0.9");
        assert_eq!(d.user, "sa");
        assert_eq!(d.password, "p@ss:w/rd");
        assert_eq!(d.database, "Mir2");
        assert_eq!(d.port, 3306);
        assert_eq!(
            d.to_sqlx_url(),
            "mysql://sa:p%40ss%3Aw%2Frd@10.0.0.9:3306/Mir2"
        );
        let p = LegacyDsn::parse("server=h;port=13306;uid=u;pwd=p;database=d;").unwrap();
        assert_eq!(p.port, 13306);
    }

    /// 缺关键键 / 端口非法必须报错（M1④），不静默取默认。
    #[test]
    fn rejects_incomplete_or_bad() {
        assert!(LegacyDsn::parse("uid=root;pwd=;database=mir2_db;").is_err());
        assert!(LegacyDsn::parse("server=127.0.0.1;uid=root;pwd=;").is_err());
        assert!(LegacyDsn::parse("server=h;port=abc;database=d;").is_err());
        assert!(LegacyDsn::parse("").is_err());
    }
}
