pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

pub struct Config {
    pub basic_auth: Option<BasicAuth>,
    pub host: String,
    pub port: u16,
    pub scheme: String,
}

impl Config {
    pub fn from_url(config: &str) -> Self {
        let url = <url::Url as std::str::FromStr>::from_str(config).unwrap();
        Self {
            basic_auth: if !url.username().is_empty()
                && let Some(password) = url.password()
            {
                Some(BasicAuth {
                    username: url.username().into(),
                    password: password.into(),
                })
            } else {
                None
            },
            host: url.host_str().unwrap().into(),
            port: url.port().unwrap(),
            scheme: url.scheme().into(),
        }
    }

    pub fn reqwest_proxy(&self) -> reqwest::Proxy {
        let proxy = reqwest::Proxy::all(format!(
            "{}://{}:{}",
            match self.scheme.as_str() {
                "socks5" => "socks5h", // delegate dns requests to proxy
                scheme => scheme,
            },
            self.host,
            self.port,
        ))
        .unwrap();

        match self.basic_auth {
            Some(ref a) => proxy.basic_auth(&a.username, &a.password),
            None => proxy,
        }
    }
}
