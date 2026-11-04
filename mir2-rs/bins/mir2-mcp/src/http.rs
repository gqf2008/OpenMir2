//! Streamable HTTP 传输（opt-in）：回环 bind + Bearer token，工具语义与 stdio 完全一致。
//!
//! * 传输实现：rmcp 3.8.0 的 [`StreamableHttpService`]（每次会话用工厂重建 [`Mir2Mcp`]），
//!   用 hyper 1 + `hyper_util::service::TowerToHyperService` 接到 `http1::Builder`；
//!   **不引入 axum**。
//! * 监听面：只接受回环 IP 字面量（`127.0.0.0/8` 或 `::1`）的 `ip:port`，由 CLI 层
//!   `parse_bind` 在**启动前**判定（对齐 game-svr `[Admin] AdminBind` 的口径）。端口写 `0`
//!   时由系统分配，启动行打印实际地址。
//! * 鉴权：每个请求先过 [`AuthService`]；`Authorization: Bearer <token>` 用常量时间比较，
//!   缺失/错误 ⇒ `401` + 空响应体，**不调用内层服务**（因此零触达任何工具处理器）。
//! * Host/Origin：`allowed_hosts` = rmcp 缺省（`localhost`/`127.0.0.1`/`::1`）+ **本次 bind 的
//!   IP 字面量**（否则 `127.0.0.2` 能监听却被 Host 校验 403）；其余 Host ⇒ `403`。
//!   并 `enforce_origin_validation()`（空 Origin 白名单 ⇒ 带 `Origin` 头的请求 `403`；
//!   CLI/MCP 客户端不发 Origin）。
//! * 连接层护栏：`accept()` 出错 **退避**后重试（[`ACCEPT_BACKOFF_INITIAL`] 起翻倍、
//!   [`ACCEPT_BACKOFF_MAX`] 封顶，成功一次即重置），绝不紧凑自旋；每条连接用
//!   `header_read_timeout` 限制请求头读取窗口；并发连接数由 `--max-connections` 闸门控制，
//!   超出的新连接**立即关闭**。

use std::convert::Infallible;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty};
use hyper::body::{Bytes, Incoming};
use hyper::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::sync::Semaphore;

use crate::conf::AppConfig;
use crate::server::Mir2Mcp;
use crate::HttpConfig;

/// accept 出错后的退避起点。
const ACCEPT_BACKOFF_INITIAL: Duration = Duration::from_millis(10);
/// accept 出错后的退避上界（增长到此即封顶）。
const ACCEPT_BACKOFF_MAX: Duration = Duration::from_secs(1);

/// rmcp 服务端响应体（与 `StreamableHttpService` 的 `Response` 同构）。
type HttpResponse = Response<BoxBody<Bytes, Infallible>>;

/// 可装箱的异步结果。
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// 每请求鉴权包装：token 不对 ⇒ `401`（空体、无细节），**不调用内层**。
///
/// 这就是 A4 的「零触达」承重点：401 分支在调用 `inner.call` **之前**返回。
#[derive(Clone)]
struct AuthService<S> {
    inner: S,
    token: Arc<str>,
}

impl<S> Service<Request<Incoming>> for AuthService<S>
where
    S: Service<Request<Incoming>, Response = HttpResponse, Error = Infallible>
        + Clone
        + Send
        + 'static,
    <S as Service<Request<Incoming>>>::Future: Send,
{
    type Response = HttpResponse;
    type Error = Infallible;
    type Future = BoxFuture<Result<HttpResponse, Infallible>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        if !authorized(request.headers(), &self.token) {
            let response = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(WWW_AUTHENTICATE, "Bearer")
                .body(Empty::<Bytes>::new().boxed())
                .expect("401 响应构造必须成功");
            return Box::pin(async move { Ok(response) });
        }
        let inner = self.inner.clone();
        Box::pin(async move { inner.call(request).await })
    }
}

/// 校验 `Authorization: Bearer <token>`（scheme 大小写不敏感，token 常量时间比较）。
fn authorized(headers: &hyper::HeaderMap, token: &str) -> bool {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some((scheme, presented)) = value.split_once(' ') else {
        return false;
    };
    scheme.eq_ignore_ascii_case("bearer") && tokens_equal(presented.trim(), token)
}

/// 常量时间比较：长度相等时逐字节异或累积，**不按首个不等字节提前返回**。
fn tokens_equal(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Host 白名单 = rmcp 缺省三条 + 本次 bind 的 IP 字面量（去重）。
///
/// 只放开「本次真正 bind 的地址」：`127.0.0.2` 这类回环 IP 能监听，就必须能在 `Host` 里出现，
/// 否则表现为「启动成功但每个请求 403」。非 bind 地址仍不在白名单里（fail-closed）。
fn allowed_hosts(bind_ip: IpAddr) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    let literal = bind_ip.to_string();
    if !hosts.contains(&literal) {
        hosts.push(literal);
    }
    hosts
}

/// accept 重试策略：从 [`ACCEPT_BACKOFF_INITIAL`] 起翻倍，[`ACCEPT_BACKOFF_MAX`] 封顶；
/// 成功 accept 一次后重置为起点。
#[derive(Debug, Clone)]
struct AcceptBackoff {
    next: Duration,
    max: Duration,
}

impl Default for AcceptBackoff {
    fn default() -> Self {
        Self {
            next: ACCEPT_BACKOFF_INITIAL,
            max: ACCEPT_BACKOFF_MAX,
        }
    }
}

impl AcceptBackoff {
    /// 取本次错误后应等待的时长，并推进到下一档（封顶）。
    fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = (self.next * 2).min(self.max);
        delay
    }

    /// accept 成功后回到起点（一次故障不应让后续连接一直按上界等待）。
    fn on_success(&mut self) {
        self.next = ACCEPT_BACKOFF_INITIAL;
    }
}

/// accept 循环骨架：`accept` 给出下一次结果，`handle` 处置连接，`sleep` 执行退避等待。
///
/// 三者都是注入的，生产与单测共用**同一份循环**——这样「出错后必须退避而不是紧凑 `continue`」
/// 才能被单测直接钉住（`sleep` 的调用次数与时长序列）。
///
/// * `accept` 返回 `None` ⇒ 结束循环（生产 listener 永不为 `None`，只有单测脚本会终止）；
/// * `Some(Ok(connection))` ⇒ 重置退避并交给 `handle`；
/// * `Some(Err(error))` ⇒ **先等待退避时长**（`sleep`）再继续。
async fn accept_loop<T, A, FutA, H, Z, FutZ>(mut accept: A, mut handle: H, mut sleep: Z)
where
    A: FnMut() -> FutA,
    FutA: Future<Output = Option<std::io::Result<T>>>,
    H: FnMut(T),
    Z: FnMut(Duration) -> FutZ,
    FutZ: Future<Output = ()>,
{
    let mut backoff = AcceptBackoff::default();
    loop {
        let Some(result) = accept().await else {
            return;
        };
        match result {
            Ok(connection) => {
                backoff.on_success();
                handle(connection);
            }
            Err(error) => {
                let delay = backoff.next_delay();
                eprintln!(
                    "mir2-mcp: accept 失败：{error}（{delay:?} 后重试，上界 {ACCEPT_BACKOFF_MAX:?}）"
                );
                sleep(delay).await;
            }
        }
    }
}

/// 起 HTTP 服务（只监听已校验的回环地址），直到进程结束。
///
/// # Errors
///
/// 监听失败（端口被占、权限等）返回人读错误；调用方打印后以非零退出。
pub async fn serve(config: HttpConfig) -> Result<(), String> {
    let listener = Arc::new(
        tokio::net::TcpListener::bind(config.bind)
            .await
            .map_err(|e| format!("无法监听 {}：{e}", config.bind))?,
    );
    let local = listener
        .local_addr()
        .map_err(|e| format!("读取监听地址失败：{e}"))?;
    // 启动行只暴露地址与护栏参数；token 不打印、不进日志。
    // 端口 0 时这里是系统分配后的**实际端口**（e2e 解析此行，避免先绑后放）。
    eprintln!(
        "mir2-mcp: HTTP 传输监听 http://{local}/mcp（只回环，Bearer token 已配置；并发上限 {}，请求头超时 {}ms）",
        config.max_connections,
        config.header_timeout.as_millis()
    );

    let app: AppConfig = config.app;
    let factory = move || Ok::<Mir2Mcp, std::io::Error>(Mir2Mcp::new(app.clone()));
    let http_config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts(local.ip()))
        .enforce_origin_validation();
    let mcp = StreamableHttpService::new(
        factory,
        Arc::new(LocalSessionManager::default()),
        http_config,
    );
    let inner = TowerToHyperService::new(mcp);
    let token: Arc<str> = Arc::from(config.token.as_str());
    let max_connections = config.max_connections;
    let header_timeout = config.header_timeout;
    let permits = Arc::new(Semaphore::new(max_connections));

    let accept = {
        let listener = Arc::clone(&listener);
        move || {
            let listener = Arc::clone(&listener);
            async move { Some(listener.accept().await) }
        }
    };
    let handle = move |(stream, _peer): (tokio::net::TcpStream, std::net::SocketAddr)| {
        // 并发闸门：拿不到许可 ⇒ 立即关闭（不排队、不 spawn），既有连接不受影响。
        let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
            eprintln!("mir2-mcp: 并发连接已达上限 {max_connections}，立即关闭新连接");
            drop(stream);
            return;
        };
        let service = AuthService {
            inner: inner.clone(),
            token: Arc::clone(&token),
        };
        tokio::spawn(async move {
            // 许可随连接生命周期持有（keep-alive 连接在关闭前一直占一个名额）。
            let _permit = permit;
            let io = TokioIo::new(stream);
            let mut builder = hyper::server::conn::http1::Builder::new();
            builder
                .timer(TokioTimer::new())
                .header_read_timeout(header_timeout);
            if let Err(error) = builder.serve_connection(io, service).await {
                // 客户端提前断开 / 请求头超时都是常态，不视为服务端故障。
                eprintln!("mir2-mcp: HTTP 连接结束：{error}");
            }
        });
    };
    accept_loop(accept, handle, tokio::time::sleep).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(value: Option<&str>) -> hyper::HeaderMap {
        let mut headers = hyper::HeaderMap::new();
        if let Some(value) = value {
            headers.insert(AUTHORIZATION, value.parse().expect("header value"));
        }
        headers
    }

    #[test]
    fn token_comparison_is_exact() {
        assert!(tokens_equal("s3cret", "s3cret"));
        assert!(!tokens_equal("s3cret", "s3creT"));
        assert!(!tokens_equal("s3cret", "s3cret "), "长度不同必须不等");
        assert!(!tokens_equal("", "x"));
        assert!(
            tokens_equal("", ""),
            "空 token 的相等语义由 CLI 层拒绝，不在比较器里特判"
        );
    }

    #[test]
    fn authorization_header_is_required() {
        assert!(!authorized(&headers_with(None), "t"));
        assert!(authorized(&headers_with(Some("Bearer t")), "t"));
        assert!(
            authorized(&headers_with(Some("bearer t")), "t"),
            "scheme 大小写不敏感"
        );
        assert!(!authorized(&headers_with(Some("Bearer  tt")), "t"));
        assert!(!authorized(&headers_with(Some("Bearer t")), "other"));
        assert!(!authorized(&headers_with(Some("Basic t")), "t"));
        assert!(!authorized(&headers_with(Some("Bearert")), "t"));
    }

    /// F1 的注入点：bind 的 IP 字面量必须进 `allowed_hosts`，且不挤掉 rmcp 缺省三条、不重复。
    #[test]
    fn allowed_hosts_includes_bind_ip_literal() {
        let hosts = allowed_hosts("127.0.0.2".parse().expect("ip"));
        assert!(
            hosts.iter().any(|host| host == "127.0.0.2"),
            "bind IP 必须进白名单，否则 127.0.0.2 每请求 403：{hosts:?}"
        );
        for default in ["localhost", "127.0.0.1", "::1"] {
            assert!(
                hosts.iter().any(|host| host == default),
                "rmcp 缺省 {default} 不应被挤掉：{hosts:?}"
            );
        }

        let hosts = allowed_hosts("127.0.0.1".parse().expect("ip"));
        assert_eq!(
            hosts.iter().filter(|host| *host == "127.0.0.1").count(),
            1,
            "缺省已含的地址不重复追加：{hosts:?}"
        );

        let hosts = allowed_hosts("::1".parse().expect("ip"));
        assert_eq!(
            hosts.iter().filter(|host| *host == "::1").count(),
            1,
            "IPv6 回环同样去重：{hosts:?}"
        );
    }

    /// A2：错误路径必须**退避等待**而不是紧凑 `continue`——用注入的 `accept`/`sleep`
    /// 直接驱动生产循环，数 sleep 次数与时长序列（不靠墙钟）。
    #[tokio::test]
    async fn accept_errors_back_off_instead_of_spinning() {
        use std::collections::VecDeque;

        // 脚本：3 连错误 → 1 次成功 → 1 次错误 → 结束（None）。
        let mut script: VecDeque<Option<std::io::Result<u8>>> = VecDeque::from(vec![
            Some(Err(std::io::Error::from_raw_os_error(24))), // EMFILE 形态
            Some(Err(std::io::Error::from_raw_os_error(24))),
            Some(Err(std::io::Error::from_raw_os_error(24))),
            Some(Ok(7)),
            Some(Err(std::io::Error::from_raw_os_error(24))),
            None,
        ]);
        let mut accept_calls = 0usize;
        let mut handled: Vec<u8> = Vec::new();
        let mut sleeps: Vec<Duration> = Vec::new();

        accept_loop(
            || {
                accept_calls += 1;
                let next = script.pop_front().flatten();
                std::future::ready(next)
            },
            |item| handled.push(item),
            |delay| {
                sleeps.push(delay);
                std::future::ready(())
            },
        )
        .await;

        assert_eq!(accept_calls, 6, "每次循环都真的去 accept 了");
        assert_eq!(handled, vec![7], "成功连接交给 handle 且只交一次");
        assert_eq!(
            sleeps.len(),
            4,
            "4 次错误 ⇒ 必须有 4 次退避等待（删掉 sleep 就是紧凑忙转）"
        );
        for (index, delay) in sleeps.iter().enumerate() {
            assert!(
                *delay >= ACCEPT_BACKOFF_INITIAL,
                "第 {index} 次退避不得低于起点：{delay:?}"
            );
            assert!(
                *delay <= ACCEPT_BACKOFF_MAX,
                "第 {index} 次退避不得超上界：{delay:?}"
            );
        }
        assert!(
            sleeps[0] <= sleeps[1] && sleeps[1] <= sleeps[2],
            "连续出错时退避单调不减：{sleeps:?}"
        );
        assert!(
            sleeps[2] > sleeps[0],
            "退避必须真的增长（恒等重试不算退避）：{sleeps:?}"
        );
        assert_eq!(
            sleeps[3], sleeps[0],
            "成功 accept 后必须重置到起点：{sleeps:?}"
        );
    }

    /// A2 的退化形态：持续错误（64 次）不得退化为 0 等待，且封顶在上界。
    #[test]
    fn accept_backoff_is_bounded_under_sustained_errors() {
        let mut backoff = AcceptBackoff::default();
        let mut delays = Vec::new();
        for _ in 0..64 {
            delays.push(backoff.next_delay());
        }
        for (index, delay) in delays.iter().enumerate() {
            assert!(!delay.is_zero(), "退避不得为 0（0 就是忙转）：{delay:?}");
            assert!(
                *delay <= ACCEPT_BACKOFF_MAX,
                "第 {index} 次退避必须封顶：{delay:?}"
            );
            if index > 0 {
                assert!(delays[index - 1] <= *delay, "退避必须单调不减：{delays:?}");
            }
        }
        assert_eq!(
            *delays.last().expect("非空"),
            ACCEPT_BACKOFF_MAX,
            "持续错误最终停在上界"
        );
    }
}
