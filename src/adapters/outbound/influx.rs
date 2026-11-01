use std::time::Duration;

use aquarium_core::secret::Secret;

#[derive(Debug)]
pub struct InfluxSettings {
    pub url: &'static str,
    pub org: &'static str,
    pub bucket: &'static str,
    pub token: Secret<&'static str>,
    pub buffer_capacity: usize,
    pub batch_max_snapshots: usize,
    pub retry_backoff: Duration,
    pub request_timeout: Duration,
    pub worker_stack_size: usize,
}

#[cfg(target_os = "espidf")]
pub use hw::InfluxWriter;

#[cfg(target_os = "espidf")]
mod hw {
    use std::ffi::CStr;
    use std::time::Duration;

    use esp_idf_hal::task::thread::ThreadSpawnConfiguration;
    use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
    use esp_idf_svc::http::Method;
    use esp_idf_svc::sys::esp_crt_bundle_attach;
    use tokio::sync::{mpsc, oneshot};
    use tracing::{error, warn};

    use aquarium_core::ports::outbound::metrics_export::{ExportError, MetricsWriter};

    use super::InfluxSettings;

    type Job = (String, oneshot::Sender<Result<(), ExportError>>);

    pub struct InfluxWriter {
        jobs: mpsc::Sender<Job>,
    }

    struct WorkerConfig {
        url: String,
        authorization: String,
        request_timeout: Duration,
    }

    impl InfluxWriter {
        pub fn spawn(settings: &InfluxSettings) -> anyhow::Result<Self> {
            let (tx, rx) = mpsc::channel::<Job>(1);

            let config = WorkerConfig {
                url: format!(
                    "{}/api/v2/write?org={}&bucket={}&precision=s",
                    settings.url.trim_end_matches('/'),
                    settings.org,
                    settings.bucket,
                ),
                authorization: format!("Token {}", settings.token.expose()),
                request_timeout: settings.request_timeout,
            };

            ThreadSpawnConfiguration {
                name: Some(CStr::from_bytes_with_nul(b"influx\0").expect("static name")),
                stack_size: settings.worker_stack_size,
                ..Default::default()
            }
            .set()?;

            std::thread::Builder::new().spawn(move || worker(config, rx))?;

            ThreadSpawnConfiguration::default().set()?;

            Ok(Self { jobs: tx })
        }
    }

    impl MetricsWriter for InfluxWriter {
        async fn write(&self, body: &str) -> Result<(), ExportError> {
            let (reply, answer) = oneshot::channel();

            self.jobs
                .send((body.to_owned(), reply))
                .await
                .map_err(|_| ExportError::Transport("the Influx worker thread is gone"))?;

            answer
                .await
                .map_err(|_| ExportError::Transport("the Influx worker dropped the request"))?
        }
    }

    fn worker(config: WorkerConfig, mut rx: mpsc::Receiver<Job>) {
        let WorkerConfig {
            url,
            authorization,
            request_timeout,
        } = config;

        let mut connection: Option<EspHttpConnection> = None;

        while let Some((body, reply)) = rx.blocking_recv() {
            let result = match connection.take() {
                Some(open) => post(open, &url, &authorization, &body),
                None => match connect(request_timeout) {
                    Ok(fresh) => post(fresh, &url, &authorization, &body),
                    Err(error) => {
                        error!(%error, "failed to open an Influx connection");
                        (None, Err(ExportError::Transport("connection setup failed")))
                    }
                },
            };

            let (reusable, outcome) = result;
            connection = reusable;

            if let Err(error) = &outcome {
                warn!(%error, "Influx write failed");
            }

            let _ = reply.send(outcome);
        }
    }

    fn connect(timeout: Duration) -> Result<EspHttpConnection, esp_idf_svc::sys::EspError> {
        EspHttpConnection::new(&Configuration {
            timeout: Some(timeout),
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        })
    }

    fn post(
        mut connection: EspHttpConnection,
        url: &str,
        authorization: &str,
        body: &str,
    ) -> (Option<EspHttpConnection>, Result<(), ExportError>) {
        let length = body.len().to_string();
        let headers = [
            ("Authorization", authorization),
            ("Content-Type", "text/plain; charset=utf-8"),
            ("Content-Length", length.as_str()),
        ];

        if connection
            .initiate_request(Method::Post, url, &headers)
            .is_err()
        {
            return (None, Err(ExportError::Transport("request setup failed")));
        }

        if connection.write_all(body.as_bytes()).is_err() {
            return (None, Err(ExportError::Transport("body write failed")));
        }

        if connection.initiate_response().is_err() {
            return (None, Err(ExportError::Transport("no response")));
        }

        let status = connection.status();
        (Some(connection), classify(status))
    }

    fn classify(status: u16) -> Result<(), ExportError> {
        match status {
            200..=299 => Ok(()),
            429 => Err(ExportError::Unavailable(status)),
            400..=499 => Err(ExportError::Rejected(status)),
            _ => Err(ExportError::Unavailable(status)),
        }
    }
}
