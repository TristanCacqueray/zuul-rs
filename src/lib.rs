#![warn(missing_docs)]
#![crate_name = "zuul"]

//! # Zuul
//!
//! `zuul` is a client library to interface with [zuul-ci](https://zuul-ci.org).

use async_stream::stream;
use chrono::{DateTime, Utc};
use futures_core::stream::Stream;
use futures_util::StreamExt;
use log::{debug, error};
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashSet;
use std::thread;
use std::time::Duration;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;
use url::{ParseError, Url};

/// The client
pub struct Zuul {
    client: reqwest::Client,
    api: Url,
}

/// Parse the api root url, ensuring it is slash terminated to enable Path::join
fn parse_root_url(url: &str) -> Result<Url, ParseError> {
    let mut url = Url::parse(url)?;
    if url.path().chars().last().unwrap() != '/' {
        let new_path = format!("{}/", String::from(url.path()));
        url.set_path(&new_path);
    }
    Ok(url)
}

/// Helper function to validate the api url and creates a client
pub fn create_client(api: &str) -> Result<Zuul, ParseError> {
    let url = parse_root_url(api)?;
    Ok(Zuul::new(url))
}

impl Zuul {
    /// Create a new client
    pub fn new(api: Url) -> Self {
        Zuul {
            client: reqwest::Client::new(),
            api,
        }
    }

    /// Produce a continuous stream of unique build.
    pub fn builds_tail(
        &self,
        loop_delay: Duration,
        since: Option<String>,
    ) -> impl Stream<Item = Build> + '_ {
        let mut since = since.clone();
        stream! {
            loop {
                match since.clone() {
                    Some(uuid) => {
                        for await (idx, build) in self.builds_stream().enumerate() {
                            if (idx == 0) {
                                since = Some(build.uuid.clone());
                            }
                            match &build.uuid[..] == uuid {
                                true => break,
                                false => yield build
                            }
                        }
                    },
                    None => {
                        // get latest build
                        let mut builds = self.builds(0, 1).await.unwrap();
                        if let Some(Ok(build)) = builds.pop() {
                            debug!("Current latest build is {:?}", build);
                            since = Some(build.uuid.clone());
                        }
                        if let None = since {
                            panic!("Could not get the latest build");
                        }
                    }
                }
                debug!("Now sleeping {:?}", loop_delay);
                thread::sleep(loop_delay);
            }
        }
    }

    /// Produce a stream of unique build.
    pub fn builds_stream(&self) -> impl Stream<Item = Build> + '_ {
        let mut offset = 0;
        let mut known_builds = HashSet::new();
        stream! {
            loop {
                let retry_strategy = ExponentialBackoff::from_millis(10).max_delay(Duration::from_secs(13))
                    .map(jitter).take(10);
                let action = || self.builds(offset, 20);
                let builds = Retry::spawn(retry_strategy, action).await.unwrap();
                offset += builds.len() as u32;
                for build_result in builds {
                    match build_result {
                        Ok(build) if known_builds.contains(&build.uuid)=> {
                            // The page moved between request, we skip the known build
                            // perhaps we should reset the offset to catchup the new one?
                        },
                        Ok(build) => {
                            // Keep track of yieled build to avoid duplicate
                            known_builds.insert(build.uuid.clone());
                            yield build;
                        },
                        Err(e) => {
                            error!("Failed to decode build: {:?}", e)
                        }
                    }
                }
            }
        }
    }

    /// Get latest builds with optional decoding error
    pub async fn builds(
        &self,
        skip: u32,
        limit: u32,
    ) -> Result<Vec<serde_json::Result<Build>>, reqwest::Error> {
        let mut url = self.api.join("builds").unwrap();
        url.query_pairs_mut()
            .append_pair("complete", "true")
            .append_pair("skip", &skip.to_string())
            .append_pair("limit", &limit.to_string());
        debug!("Querying build {}", url);
        let resp = self.client.get(url).send().await?;
        let builds: Vec<serde_json::Value> = resp.json().await?;
        Ok(builds.iter().map(|b| Build::deserialize(b)).collect())
    }

    /// Get latest builds (and panic on decoding error)
    pub async fn builds_unsafe(&self) -> Result<Vec<Build>, reqwest::Error> {
        let builds = self.builds(0, 20).await?;
        let builds: Result<Vec<Build>, _> = builds.into_iter().collect();
        Ok(builds.expect("Invalid build json"))
    }
}

/// A Build result
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Build {
    /// The build unique id
    pub uuid: String,
    /// The job name
    pub job_name: String,
    /// The job result
    pub result: String,
    /// The start time
    #[serde(with = "python_utc_without_trailing_z")]
    pub start_time: DateTime<Utc>,
    /// The end time
    #[serde(with = "python_utc_without_trailing_z")]
    pub end_time: DateTime<Utc>,
    /// The job duration in second
    #[serde(with = "rounded_float")]
    pub duration: u32,
    /// The job voting status
    pub voting: bool,
    /// The log url
    pub log_url: Option<String>,
    /// The build artifacts
    pub artifacts: Vec<Artifact>,
    /// The change's project name
    pub project: String,
    /// The change's branch name
    pub branch: String,
    /// The build pipeline
    pub pipeline: String,
    /// The change (or PR) number
    pub change: Option<u64>,
    /// The patchset number (or PR commit)
    pub patchset: Option<String>,
    /// The change ref
    #[serde(rename = "ref")]
    pub change_ref: String,
    /// The internal event id
    pub event_id: String,
}

/// A Build artifact
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Artifact {
    /// The artifact name
    pub name: String,
    /// The artifact url
    pub url: String,
}

// Copy pasta from https://serde.rs/custom-date-format.html
mod python_utc_without_trailing_z {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &'static str = "%Y-%m-%dT%H:%M:%S";

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Utc.datetime_from_str(&s, FORMAT)
            .map_err(serde::de::Error::custom)
    }
}

// For some reason, durations are sometime provided as f32, e.g. `42.0`
mod rounded_float {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &u32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(*duration)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u32, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = f32::deserialize(deserializer)?;
        Ok(v as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDateTime};
    use futures_util::pin_mut;
    use futures_util::stream::StreamExt;

    #[test]
    fn it_parse_url() {
        let assert_url =
            |url, expected: &str| assert_eq!(parse_root_url(url).unwrap().to_string(), expected);
        assert_url("https://example.com", "https://example.com/");
        assert_url("https://example.com/", "https://example.com/");
        assert_url("https://example.com/api", "https://example.com/api/");
        assert_url("https://example.com/api/", "https://example.com/api/");
    }

    fn make_build(uuid: &str, end_time: DateTime<Utc>) -> Build {
        Build {
            uuid: String::from(uuid),
            job_name: "job".to_string(),
            result: "SUCCESS".to_string(),
            start_time: end_time + Duration::minutes(-42),
            end_time,
            duration: 42,
            voting: true,
            log_url: Some("http://localhost/".to_string() + &String::from(uuid)),
            artifacts: [].to_vec(),
            project: "project".to_string(),
            branch: "main".to_string(),
            pipeline: "check".to_string(),
            change: Some(42),
            patchset: None,
            change_ref: "head".to_string(),
            event_id: "uuid".to_string(),
        }
    }

    /// Helper function to drop milli second from a DateTime so that the json encoding round trip
    fn drop_milli(dt: DateTime<Utc>) -> DateTime<Utc> {
        let ts = dt.timestamp();
        DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(ts, 0), Utc)
    }

    #[tokio::test]
    async fn it_stream_builds() {
        use env_logger;
        env_logger::init();
        use httpmock::prelude::*;
        let server = MockServer::start();

        let now = drop_milli(Utc::now());
        let b0 = make_build("42", now);
        let b1 = make_build("build1", now);
        let b2 = make_build("build2", now);
        let b3 = make_build("build3", now);
        // Simulate a sliding page
        let page1 = serde_json::json!([b1.clone(), b2.clone()].to_vec());
        let page2 = serde_json::json!([b2.clone(), b3.clone()].to_vec());

        let m0 = server.mock(|when, then| {
            when.method(GET).path("/builds").query_param("limit", "1");
            then.status(200).json_body(serde_json::json!([b0]));
        });
        let m1 = server.mock(|when, then| {
            when.method(GET).path("/builds").query_param("skip", "0");
            then.status(200).json_body(page1);
        });
        let m2 = server.mock(|when, then| {
            when.method(GET).path("/builds").query_param("skip", "2");
            then.status(200).json_body(page2);
        });

        let client = create_client(&server.url("/")).unwrap();
        let mut got = Vec::new();
        let s = client.builds_tail(std::time::Duration::from_millis(50), None);
        pin_mut!(s); // needed for iteration
        while let Some(build) = s.next().await {
            println!("got {:?}", build);
            got.push(build);
            if got.len() >= 3 {
                break;
            }
        }
        m0.assert();
        m1.assert();
        m2.assert();
        assert_eq!(got, [b1, b2, b3].to_vec());
    }

    #[tokio::test]
    async fn it_get_builds() {
        use httpmock::prelude::*;
        let now = drop_milli(Utc::now());
        let builds = [
            make_build("build1", now),
            make_build("build2", now + Duration::hours(-1)),
        ];

        // Start a lightweight mock server.
        let server = MockServer::start();

        // Create a mock on the server.
        let m = server.mock(|when, then| {
            when.method(GET)
                .path("/builds")
                .query_param("complete", "true");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!(builds.to_vec()));
        });

        // Get builds
        let client = create_client(&server.url("/")).unwrap();
        let got = client.builds_unsafe().await.unwrap();
        m.assert();
        assert_eq!(got, builds);
    }

    #[test]
    fn it_decodes_build() {
        let data = r#"
            {
              "uuid": "5bae5607ae964331bb5878aec0777637",
              "job_name": "hlint",
              "result": "SUCCESS",
              "start_time": "2021-10-13T12:57:20",
              "end_time": "2021-10-13T12:58:42",
              "duration": 82.0,
              "voting": true,
              "log_url": "https://softwarefactory-project.io/logs/94/22894/1/gate/hlint/5bae560/",
              "artifacts": [
                {
                  "name": "Zuul Manifest",
                  "url": "https://softwarefactory-project.io/logs/94/22894/1/gate/hlint/5bae560/zuul-manifest.json",
                  "metadata": {
                    "type": "zuul_manifest"
                  }
                },
                {
                  "name": "HLint report",
                  "url": "https://softwarefactory-project.io/logs/94/22894/1/gate/hlint/5bae560/hlint.html"
                }
              ],
              "project": "software-factory/matrix-client-haskell",
              "branch": "master",
              "pipeline": "gate",
              "change": 22894,
              "patchset": "1",
              "ref": "refs/changes/94/22894/1",
              "ref_url": "https://softwarefactory-project.io/r/22894",
              "event_id": "40d9b63d749c48eabb3d7918cfab0d31"
            }"#;
        let build: Build = serde_json::from_str(data).unwrap();
        assert_eq!(build.uuid, "5bae5607ae964331bb5878aec0777637");
    }
}
