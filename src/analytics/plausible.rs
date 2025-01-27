use crate::analytics::Analytics;
use crate::settings::Settings;
use anyhow::Error;
use log::{debug, warn};
use nostr::serde_json;
use reqwest::ClientBuilder;
use rocket::Request;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

#[derive(Debug, Serialize, Deserialize)]
struct Event {
    pub name: String,
    pub domain: String,
    pub url: String,
    pub referrer: Option<String>,
    #[serde(skip_serializing)]
    pub user_agent: Option<String>,
    #[serde(skip_serializing)]
    pub xff: Option<String>,
}

pub struct PlausibleAnalytics {
    tx: UnboundedSender<Event>,
}

impl PlausibleAnalytics {
    pub fn new(settings: &Settings) -> Self {
        let (tx, mut rx) = unbounded_channel::<Event>();
        let url = match &settings.plausible_url {
            Some(s) => s.clone(),
            _ => "".to_string(),
        };
        let pub_url = settings.public_url.clone();
        let c = ClientBuilder::new().build().unwrap();
        tokio::spawn(async move {
            while let Some(mut msg) = rx.recv().await {
                msg.url = format!("{}{}", pub_url, msg.url);

                let body = serde_json::to_string(&msg).unwrap();
                match c
                    .post(format!("{}/api/event", url))
                    .header(
                        "user-agent",
                        match &msg.user_agent {
                            Some(s) => s,
                            None => "",
                        },
                    )
                    .header(
                        "x-forwarded-for",
                        match &msg.xff {
                            Some(s) => s,
                            None => "",
                        },
                    )
                    .header("content-type", "application/json")
                    .body(body)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await
                {
                    Ok(_v) => debug!("Sent {:?}", msg),
                    Err(e) => warn!("Failed to track: {}", e),
                }
            }
        });

        Self { tx }
    }
}

impl Analytics for PlausibleAnalytics {
    fn track(&self, req: &Request) -> Result<(), Error> {
        Ok(self.tx.send(Event {
            name: "pageview".to_string(),
            domain: match req.host() {
                Some(s) => s.to_string(),
                None => return Ok(()), // ignore request
            },
            url: req.uri().to_string(),
            referrer: req.headers().get_one("Referer").map(|s| s.to_string()),
            user_agent: req.headers().get_one("User-Agent").map(|s| s.to_string()),
            xff: req
                .headers()
                .get_one("X-Forwarded-For")
                .map(|s| s.to_string()),
        })?)
    }
}
