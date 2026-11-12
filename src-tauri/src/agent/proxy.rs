use reqwest::{Client, Proxy, Url};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Configuration {
    pub enabled: bool,
    pub url: String,
}

impl Configuration {
    pub fn is_default(&self) -> bool {
        !self.enabled && self.url.is_empty()
    }

    pub fn load_local(service: &str) -> Result<Self, String> {
        let config: Self =
            crate::credentials::read(service, crate::credentials::LOCAL_TOOL_PROXY_KEY)?
                .map(|value| serde_json::from_str(&value).map_err(|_| "本机工具代理配置格式有误。"))
                .transpose()?
                .unwrap_or_default();
        config.validate()?;
        Ok(config)
    }

    pub fn save_local(&self, service: &str) -> Result<(), String> {
        self.validate()?;
        let value = serde_json::to_string(self).map_err(|_| "无法编码本机工具代理配置。")?;
        crate::credentials::save(service, crate::credentials::LOCAL_TOOL_PROXY_KEY, &value)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let invalid = || {
            "工具代理地址无效，请填写 HTTP(S)、socks5:// 或 socks5h:// 代理地址与端口，且不包含路径、查询或片段。".to_string()
        };
        if self.url.len() > 4096 {
            return Err(invalid());
        }
        let url = Url::parse(self.url.trim()).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
            || url.host_str().is_none()
            || url.port_or_known_default().is_none_or(|port| port == 0)
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid());
        }
        Proxy::all(url.as_str()).map_err(|_| invalid())?;
        Ok(())
    }

    /// Keep the existing default network behavior when disabled. An enabled proxy
    /// is explicit, including DNS-over-SOCKS for socks5h, without an environment
    /// NO_PROXY override or a direct-connection fallback.
    pub fn client(&self, default: &Client) -> Result<Client, String> {
        self.validate()?;
        if !self.enabled {
            return Ok(default.clone());
        }
        let proxy = Proxy::all(self.url.trim()).map_err(|_| "无法创建工具代理配置。")?;
        Client::builder()
            .no_proxy()
            .proxy(proxy)
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(600))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "无法创建工具代理客户端，请检查代理配置。".into())
    }
}

#[cfg(test)]
mod tests;
