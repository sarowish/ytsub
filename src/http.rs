use crate::CONFIG;
use anyhow::{Context, Result, ensure};
use reqwest::{Client, NoProxy, Proxy, Url};
use std::time::Duration;

pub fn client() -> Result<Client> {
    let mut builder = Client::builder().timeout(Duration::from_secs(CONFIG.request_timeout));

    if let Some(proxy_url) = CONFIG.proxy.as_deref() {
        let proxy_url = Url::parse(proxy_url).context("invalid URL in `proxy` configuration")?;
        ensure!(
            matches!(
                proxy_url.scheme(),
                "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
            ),
            "unsupported URL scheme in `proxy` configuration"
        );
        ensure!(
            proxy_url.has_host(),
            "missing host in `proxy` configuration"
        );

        let proxy = Proxy::all(proxy_url)
            .context("invalid URL in `proxy` configuration")?
            .no_proxy(NoProxy::from_env());
        builder = builder.proxy(proxy);
    }

    builder.build().context("failed to build HTTP client")
}
