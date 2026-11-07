//! `PlanesClient` 的纯状态机（G11 / A3）。
//!
//! 分服（`ServerIndex != 0`）连主服。与 [`crate::server`] 的差异有两处，都照 oracle 保留：
//!
//! - 解码循环条件是 `IndexOf(')') > -1`（服务器是 `> 0`）；
//! - `Run()` 只在**在线**时解码；断线期间收到的字节留在缓冲里，重连后**继续**累积。
//!
//! 客户端**没有**任何转发（C# 代码里不存在"把帧发给别的服务器"的路径）。

use crate::{message, Decoder, Flavor, Frame};

/// `tick` 的产出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// 本地处理成功的一帧。
    Frame(Frame),
    /// oracle 在 `ProcessData` 里 `throw`（211）⇒ `DecodeSocStr` 的 `catch` 记
    /// `[Exception] TFrmSrvMsg::DecodeSocStr` 并丢弃本批剩余帧与缓冲余量。
    ///
    /// 这一条是**客户端与服务器的可观察差异**：服务器静默，客户端要写日志
    /// （日志由传输层负责，本 crate 不写 stdout/stderr）。
    Aborted,
}

/// 分服的跨服状态机。
#[derive(Debug)]
pub struct Client {
    decoder: Decoder,
    online: bool,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// 新建（离线）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            decoder: Decoder::new(Flavor::Client),
            online: false,
        }
    }

    /// 传输层通知连接状态变化（对应 `TouchSocket` 的 `Connected` / `Disconnected` 事件）。
    pub fn set_online(&mut self, online: bool) {
        self.online = online;
    }

    /// 当前是否在线（C# `_tcpClient.Online`）。
    #[must_use]
    pub fn is_online(&self) -> bool {
        self.online
    }

    /// 累积收到的字节。**离线也累积**（oracle 的 `MsgClientRead` 不看 Online）。
    pub fn received(&mut self, chunk: &[u8]) {
        self.decoder.push(chunk);
    }

    /// 对应 oracle `Run()`：**仅在线时**解码。
    pub fn tick(&mut self) -> Vec<Event> {
        if !self.online {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (_, frame) in self.decoder.decode_raw() {
            if message::process(frame.ident).is_err() {
                self.decoder.clear();
                out.push(Event::Aborted);
                break;
            }
            out.push(Event::Frame(frame));
        }
        out
    }

    /// 对应 oracle `CheckConnected()`：**只在 `nServerNumber > 0` 时**才重连。
    #[must_use]
    pub fn should_reconnect(&self, server_number: i32) -> bool {
        server_number > 0 && !self.online
    }

    /// 当前缓冲区（测试与排障用）。
    #[must_use]
    pub fn buffered(&self) -> &[u8] {
        self.decoder.buffered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr(ident: i32, server_num: i32, body: &[u8]) -> Frame {
        Frame {
            ident,
            server_num,
            body: body.to_vec(),
        }
    }

    #[test]
    fn offline_bytes_are_buffered_but_not_decoded() {
        let mut c = Client::new();
        c.received(b"(201/9/a)");
        assert_eq!(c.tick(), Vec::new(), "离线时 Run() 不解码");
        assert_eq!(c.buffered(), b"(201/9/a)", "缓冲保留");
        c.set_online(true);
        assert_eq!(c.tick(), vec![Event::Frame(fr(201, 9, b"a"))]);
    }

    #[test]
    fn client_loop_condition_differs_from_server() {
        // A2-2 的 oracle 帧表：(1/2/a))(3/4/y) 在 client 下解出两帧（server 只解一帧）。
        let mut c = Client::new();
        c.set_online(true);
        c.received(b"(1/2/a))(3/4/y)");
        assert_eq!(
            c.tick(),
            vec![Event::Frame(fr(1, 2, b"a")), Event::Frame(fr(3, 4, b"y")),]
        );
    }

    #[test]
    fn throw_aborts_batch_and_clears_remainder() {
        let mut c = Client::new();
        c.set_online(true);
        c.received(b"(211/0/x)(201/1/y)(202/2/z");
        assert_eq!(c.tick(), vec![Event::Aborted]);
        assert_eq!(c.buffered(), b"");
        c.received(b")");
        assert_eq!(c.tick(), Vec::new(), "被丢弃的半帧不会复活");
    }

    #[test]
    fn reconnect_only_when_server_number_positive() {
        let mut c = Client::new();
        assert!(c.should_reconnect(1), "nServerNumber=1 且离线 ⇒ 重连");
        assert!(!c.should_reconnect(0), "nServerNumber=0 ⇒ 不重连");
        assert!(!c.should_reconnect(-1), "负值同样不重连");
        c.set_online(true);
        assert!(!c.should_reconnect(1), "已在线 ⇒ 不重连");
    }
}
