//! oracle `PlanesService.Start()` 的等价物：按 `[Server] ServerIndex` 选角色。

/// 跨服角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `ServerIndex == 0`：跑 [`crate::server::Server`]（监听、主服中继）。
    Server,
    /// `ServerIndex != 0`：跑 [`crate::client::Client`]（连主服）。
    Client,
}

/// oracle `PlanesService.Start()`：`SystemShare.ServerIndex == 0` ⇒ 主服，否则分服。
///
/// `ServerIndex` 在 oracle 里是 `byte`（`SystemShare.cs`），Rust 侧配置解析已按 `u8` 校验，
/// 这里不会出现越界值。
#[must_use]
pub fn role_of(server_index: u8) -> Role {
    if server_index == 0 {
        Role::Server
    } else {
        Role::Client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_index_zero_is_the_planes_server() {
        assert_eq!(role_of(0), Role::Server);
        assert_eq!(role_of(1), Role::Client);
        assert_eq!(role_of(255), Role::Client);
    }
}
