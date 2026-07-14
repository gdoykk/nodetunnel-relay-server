use std::error::Error;
use reqwest::StatusCode;
use nodetunnel_protocol::ClientId;
use tracing::warn;
use crate::config::loader::{Config, WhitelistFailurePolicy};
use nodetunnel_protocol::packet::Packet;
use crate::relay::apps::Apps;
use crate::relay::clients::{ClientState, Clients};
use crate::relay::handlers::sender::PacketSender;
use crate::udp::common::TransferChannel;
use crate::udp::paper_interface::PaperInterface;

pub struct AuthHandler<'a> {
    udp: &'a mut PaperInterface,
    http: &'a reqwest::Client,

    clients: &'a mut Clients,
    apps: &'a mut Apps,
    config: &'a Config,
}

impl PacketSender for AuthHandler<'_> {
    fn udp_mut(&mut self) -> &mut PaperInterface {
        self.udp
    }
}

impl<'a> AuthHandler<'a> {
    pub fn new(udp: &'a mut PaperInterface,
               http: &'a reqwest::Client,
               clients: &'a mut Clients,
               apps: &'a mut Apps,
               config: &'a Config
    ) -> Self {
        Self {
            udp,
            http,
            clients,
            apps,
            config
        }
    }

    pub async fn authenticate_client(&mut self, sender_id: ClientId, app_token: &str, version: &str) {
        // Check version
        if !self.is_version_allowed(version) {
            let msg = format!("Version {version} is not allowed.");
            self.send_err(sender_id, &msg).await;
            self.force_disconnect(sender_id).await;
            return;
        }

        // Check app whitelist
        if !self.app_allowed(app_token).await {
            let msg = format!("App token {app_token} is not allowed.");
            self.send_err(sender_id, &msg).await;
            self.force_disconnect(sender_id).await;
            return;
        }

        let Some(client) = self.clients.get_mut(sender_id) else {
            warn!("attempted to authenticate a missing client {sender_id}");
            return;
        };

        let app_id = match self.apps.get_by_token(app_token) {
            Some(app) => app.id,
            None => self.apps.create(app_token.to_string())
        };

        client.state = ClientState::Authenticated { app_id };
        self.send_packet(sender_id, &Packet::ClientAuthenticated, TransferChannel::Reliable).await;
    }

    fn is_version_allowed(&self, version: &str) -> bool {
        let versions = &self.config.allowed_versions;
        versions.iter().any(|v| v == version)
    }

    async fn app_allowed(&mut self, app: &str) -> bool {
        let remote = &self.config.remote_whitelist_endpoint;
        let token = &self.config.remote_whitelist_token;

        if remote.is_empty() || token.is_empty() {
            return self.check_local_whitelist(app);
        }

        match self.check_remote_whitelist(remote, app, token).await {
            Ok(res) => res,
            Err(e) => match self.config.whitelist_failure_policy {
                WhitelistFailurePolicy::FailClosed => {
                    warn!("remote whitelist check failed, rejecting (fail_closed policy): {e}");
                    false
                }
                WhitelistFailurePolicy::FailOpenToLocal => {
                    warn!("remote whitelist check failed, falling back to local whitelist (fail_open_to_local policy): {e}");
                    self.check_local_whitelist(app)
                }
            },
        }
    }

    fn check_local_whitelist(&self, app: &str) -> bool {
        let whitelist = &self.config.whitelist;

        if whitelist.is_empty() {
            true
        } else {
            whitelist.iter().any(|w| w == app)
        }
    }

    async fn check_remote_whitelist(
        &self,
        endpoint: &str,
        app: &str,
        relay_token: &str,
    ) -> Result<bool, Box<dyn Error>> {
        let url = format!("{endpoint}/{app}");

        let res = self.http
            .get(&url)
            .header("X-Relay-Token", relay_token)
            .send()
            .await?;

        match res.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            s => Err(format!("unexpected status from endpoint: {s}").into()),
        }
    }
}
