//! The manual smoke examples inherit the executor proxy and CA trust.

use std::sync::Arc;

struct SmokeNetwork(ncd_network::NetworkSnapshot);

impl ncd_network::NetworkProvider for SmokeNetwork {
    fn snapshot(&self) -> ncd_network::NetworkSnapshot {
        self.0.clone()
    }
}

pub fn configure_network() -> Result<(), Box<dyn std::error::Error>> {
    let bundle = std::env::var_os("SSL_CERT_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/etc/ssl/certs/ca-certificates.crt".into());
    let mut builder = reqwest::Client::builder()
        .user_agent("NapCatQQ-Desktop-Linux-Install-Smoke")
        .connect_timeout(std::time::Duration::from_secs(15))
        .gzip(false);
    for certificate in reqwest::Certificate::from_pem_bundle(&std::fs::read(bundle)?)? {
        builder = builder.add_root_certificate(certificate);
    }
    ncd_network::install_network_provider(Arc::new(SmokeNetwork(ncd_network::NetworkSnapshot {
        client: builder.build()?,
        settings: ncd_domain::NetworkSettings::default(),
    })))?;
    Ok(())
}
