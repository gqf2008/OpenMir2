//! Streamable HTTP 传输（opt-in）：回环 bind + Bearer token，工具语义与 stdio 完全一致。
//!
//! * 传输实现：rmcp 3.8.0 的 [`StreamableHttpService`]（每次会话用工厂重建 [`Mir2Mcp`]），
//!   用 hyper 1 + `hyper_util::service::TowerToHyperService` 接到 `http1::Builder`；
//!   **不引入 axum**。
//! * 监听面：只接受回环 IP 字面量（`127.0.0.0/8` 或 `::1`）的 `ip:port`，由 CLI 层
//!   `parse_bind` 在**启动前**判定（对齐 game-svr `[Admin] AdminBind` 的口径）。
//! * 鉴权：每个请求先过 [`AuthService`]；`Authorization: Bearer <token>` 用常量时间比较，
//!   缺失/错误 ⇒ `401` + 空响应体，**不调用内层服务**（因此零触达任何工具处理器）。
//! * Host/Origin：保留 rmcp 缺省 `allowed_hosts`（`localhost`/`127.0.0.1`/`::1`），
//!   并 `enforce_origin_validation()`（空 Origin 白名单 ⇒ 带 `Origin` 头的请求被拒；
//!   CLI/MCP 客户端不发 Origin）。

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty};
use hyper::body::{Bytes, Incoming};
use hyper::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::conf::AppConfig;
use crate::server::Mir2Mcp;
use crate::HttpConfig;

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

/// 起 HTTP 服务（只监听已校验的回环地址），直到进程结束。
///
/// # Errors
///
/// 监听失败（端口被占、权限等）返回人读错误；调用方打印后以非零退出。
pub async fn serve(config: HttpConfig) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .map_err(|e| format!("无法监听 {}：{e}", config.bind))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("读取监听地址失败：{e}"))?;
    // 启动行只暴露地址；token 不打印、不进日志。
    eprintln!("mir2-mcp: HTTP 传输监听 http://{local}/mcp（只回环，Bearer token 已配置）");

    let app: AppConfig = config.app;
    let factory = move || Ok::<Mir2Mcp, std::io::Error>(Mir2Mcp::new(app.clone()));
    let http_config = StreamableHttpServerConfig::default().enforce_origin_validation();
    let mcp = StreamableHttpService::new(
        factory,
        Arc::new(LocalSessionManager::default()),
        http_config,
    );
    let inner = TowerToHyperService::new(mcp);
    let token: Arc<str> = Arc::from(config.token.as_str());

    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                eprintln!("mir2-mcp: accept 失败：{error}");
                continue;
            }
        };
        let service = AuthService {
            inner: inner.clone(),
            token: Arc::clone(&token),
        };
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            if let Err(error) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                // 客户端提前断开是常态，不视为服务端故障。
                eprintln!("mir2-mcp: HTTP 连接结束：{error}");
            }
        });
    }
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
}
