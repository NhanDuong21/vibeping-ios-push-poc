use std::{env, fs, path::PathBuf, sync::Arc};

use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64ct::{Base64UrlUnpadded, Encoding as _};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use hyper_tls::HttpsConnector;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{info, warn};
use web_push_native::{
    Auth, WebPushBuilder,
    jwt_simple::algorithms::{ECDSAP256KeyPairLike, ES256KeyPair},
    p256::PublicKey,
};

const DEFAULT_BIND: &str = "127.0.0.1:8787";
const VAPID_SUBJECT: &str = "https://github.com/NhanDuong21/vibeping-ios-push-poc";
const PRIVATE_KEY_FILE: &str = "vapid-private.key";
const SUBSCRIPTION_FILE: &str = "subscription.json";

#[derive(Debug, Parser)]
#[command(name = "vibeping-push-poc", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the PWA and same-origin Web Push API.
    Serve {
        #[arg(long, default_value = DEFAULT_BIND)]
        bind: String,
    },
    /// Send a Web Push message to the latest saved subscription.
    Send {
        #[arg(long, default_value = "VibePing")]
        title: String,
        #[arg(long, default_value = "Push from your Windows laptop works 🎉")]
        body: String,
        #[arg(long, default_value = "/")]
        url: String,
    },
    /// Show whether local VAPID material and a subscription exist.
    Status,
    /// Remove the local VAPID key and saved subscription.
    Reset,
}

#[derive(Clone, Debug)]
struct Paths {
    public_dir: PathBuf,
    data_dir: PathBuf,
}

impl Paths {
    fn discover() -> Result<Self> {
        let root = match env::var_os("VIBEPING_ROOT") {
            Some(value) => PathBuf::from(value),
            None => env::current_dir().context("could not determine the current directory")?,
        };
        Ok(Self {
            public_dir: root.join("public"),
            data_dir: root.join("data"),
        })
    }

    fn private_key(&self) -> PathBuf {
        self.data_dir.join(PRIVATE_KEY_FILE)
    }

    fn subscription(&self) -> PathBuf {
        self.data_dir.join(SUBSCRIPTION_FILE)
    }
}

#[derive(Clone)]
struct AppState {
    paths: Paths,
    vapid_public_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PushSubscription {
    endpoint: String,
    #[serde(rename = "expirationTime", skip_serializing_if = "Option::is_none")]
    expiration_time: Option<i64>,
    keys: SubscriptionKeys,
    #[serde(default = "Utc::now")]
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct TestNotification {
    title: String,
    body: String,
    tag: String,
    url: String,
}

impl Default for TestNotification {
    fn default() -> Self {
        Self {
            title: "VibePing".to_owned(),
            body: "Push from your Windows laptop works 🎉".to_owned(),
            tag: "vibeping-test".to_owned(),
            url: "/".to_owned(),
        }
    }
}

#[derive(Debug, Serialize)]
struct PushDelivery {
    delivered_to_provider: bool,
    provider_status: u16,
    provider_reason: String,
    stale_subscription: bool,
    message: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(error: anyhow::Error) -> Self {
        warn!(error = %error, "API request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vibeping_push_poc=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let paths = Paths::discover()?;

    match cli.command {
        Command::Serve { bind } => serve(paths, &bind).await,
        Command::Send { title, body, url } => {
            let notification = TestNotification {
                title,
                body,
                url,
                ..TestNotification::default()
            };
            let delivery = send_push(&paths, notification).await?;
            print_delivery(&delivery);
            if delivery.delivered_to_provider {
                Ok(())
            } else {
                bail!(delivery.message)
            }
        }
        Command::Status => show_status(&paths),
        Command::Reset => reset(&paths),
    }
}

async fn serve(paths: Paths, bind: &str) -> Result<()> {
    if !paths.public_dir.join("index.html").is_file() {
        bail!(
            "PWA assets were not found at {}. Run this command from the repository root.",
            paths.public_dir.display()
        );
    }

    let key_pair = create_or_load_vapid(&paths)?;
    let state = Arc::new(AppState {
        vapid_public_key: public_key_base64(&key_pair),
        paths: paths.clone(),
    });

    let api = Router::new()
        .route("/health", get(api_health))
        .route("/status", get(api_status))
        .route("/vapid-public-key", get(api_vapid_public_key))
        .route("/subscription", post(api_save_subscription))
        .route("/test-push", post(api_test_push));

    let static_files = ServeDir::new(&paths.public_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeDir::new(&paths.public_dir));

    let app = Router::new()
        .nest("/api", api)
        .route("/sw.js", get(service_worker))
        .fallback_service(static_files)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("could not bind the local server to {bind}"))?;
    info!(url = %format!("http://{bind}"), "VibePing Push PoC server ready");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server stopped unexpectedly")?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        warn!(%error, "failed to install Ctrl+C handler");
    }
}

async fn api_health() -> impl IntoResponse {
    no_store(Json(json!({
        "status": "ok",
        "service": "vibeping-push-poc"
    })))
}

async fn api_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let subscription = load_subscription(&state.paths).ok();
    no_store(Json(json!({
        "server": "connected",
        "vapid_ready": true,
        "subscription_active": subscription.is_some(),
        "subscription_created_at": subscription.map(|value| value.created_at),
    })))
}

async fn api_vapid_public_key(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    no_store(Json(json!({ "publicKey": state.vapid_public_key })))
}

async fn api_save_subscription(
    State(state): State<Arc<AppState>>,
    Json(mut subscription): Json<PushSubscription>,
) -> Result<impl IntoResponse, ApiError> {
    validate_subscription(&subscription).map_err(ApiError::bad_request)?;
    subscription.created_at = Utc::now();
    save_subscription(&state.paths, &subscription).map_err(ApiError::internal)?;
    info!("saved the latest Web Push subscription");
    Ok(no_store((
        StatusCode::CREATED,
        Json(json!({
            "saved": true,
            "created_at": subscription.created_at,
        })),
    )))
}

async fn api_test_push(
    State(state): State<Arc<AppState>>,
    Json(notification): Json<TestNotification>,
) -> Result<impl IntoResponse, ApiError> {
    let delivery = send_push(&state.paths, notification)
        .await
        .map_err(ApiError::internal)?;
    let status = if delivery.delivered_to_provider {
        StatusCode::OK
    } else {
        StatusCode::BAD_GATEWAY
    };
    Ok(no_store((status, Json(delivery))))
}

async fn service_worker(State(state): State<Arc<AppState>>) -> Result<Response, ApiError> {
    let body = fs::read(state.paths.public_dir.join("sw.js"))
        .context("could not read public/sw.js")
        .map_err(ApiError::internal)?;
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("service-worker-allowed"),
        HeaderValue::from_static("/"),
    );
    Ok(response)
}

fn no_store<T: IntoResponse>(value: T) -> Response {
    let mut response = value.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn create_or_load_vapid(paths: &Paths) -> Result<ES256KeyPair> {
    fs::create_dir_all(&paths.data_dir)
        .with_context(|| format!("could not create {}", paths.data_dir.display()))?;
    let key_path = paths.private_key();
    if key_path.is_file() {
        let encoded = fs::read_to_string(&key_path)
            .with_context(|| format!("could not read {}", key_path.display()))?;
        let bytes = Base64UrlUnpadded::decode_vec(encoded.trim())
            .context("saved VAPID private key is not valid base64url")?;
        return ES256KeyPair::from_bytes(&bytes).context("saved VAPID private key is invalid");
    }

    let key_pair = ES256KeyPair::generate();
    let encoded = Base64UrlUnpadded::encode_string(&key_pair.to_bytes());
    fs::write(&key_path, encoded)
        .with_context(|| format!("could not persist {}", key_path.display()))?;
    info!(path = %key_path.display(), "generated and persisted a new VAPID keypair");
    Ok(key_pair)
}

fn public_key_base64(key_pair: &ES256KeyPair) -> String {
    Base64UrlUnpadded::encode_string(&key_pair.key_pair().public_key().to_bytes_uncompressed())
}

fn validate_subscription(subscription: &PushSubscription) -> Result<(), String> {
    if !subscription.endpoint.starts_with("https://") {
        return Err("push endpoint must use HTTPS".to_owned());
    }
    if subscription.keys.p256dh.trim().is_empty() || subscription.keys.auth.trim().is_empty() {
        return Err("push subscription keys are missing".to_owned());
    }
    Ok(())
}

fn save_subscription(paths: &Paths, subscription: &PushSubscription) -> Result<()> {
    fs::create_dir_all(&paths.data_dir)
        .with_context(|| format!("could not create {}", paths.data_dir.display()))?;
    let json = serde_json::to_vec_pretty(subscription)?;
    fs::write(paths.subscription(), json).context("could not persist the push subscription")
}

fn load_subscription(paths: &Paths) -> Result<PushSubscription> {
    let data = fs::read(paths.subscription()).context(
        "no saved subscription; install the PWA and tap Enable notifications on the iPhone first",
    )?;
    serde_json::from_slice(&data).context("saved subscription JSON is invalid")
}

async fn send_push(paths: &Paths, notification: TestNotification) -> Result<PushDelivery> {
    let subscription = load_subscription(paths)?;
    validate_subscription(&subscription).map_err(|message| anyhow!(message))?;
    let key_pair = create_or_load_vapid(paths)?;

    let p256dh_bytes = Base64UrlUnpadded::decode_vec(&subscription.keys.p256dh)
        .context("subscription p256dh is not valid base64url")?;
    let auth_bytes = Base64UrlUnpadded::decode_vec(&subscription.keys.auth)
        .context("subscription auth is not valid base64url")?;
    if auth_bytes.len() != 16 {
        bail!("subscription auth must decode to exactly 16 bytes");
    }
    let auth = Auth::clone_from_slice(&auth_bytes);

    let builder = WebPushBuilder::new(
        subscription
            .endpoint
            .parse()
            .context("subscription endpoint is not a valid URI")?,
        PublicKey::from_sec1_bytes(&p256dh_bytes)
            .context("subscription p256dh is not a valid P-256 public key")?,
        auth,
    )
    .with_vapid(&key_pair, VAPID_SUBJECT);

    let payload = json!({
        "title": notification.title,
        "body": notification.body,
        "tag": notification.tag,
        "timestamp": Utc::now().timestamp_millis(),
        "url": notification.url,
    });
    let request = builder
        .build(payload.to_string())
        .context("could not encrypt and sign the Web Push payload")?
        .map(Body::from);

    let https = HttpsConnector::new();
    let client = Client::builder(TokioExecutor::new()).build(https);
    let response = client
        .request(request)
        .await
        .context("could not reach the Web Push provider")?;
    let status = response.status();
    let stale = matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE);
    let delivered = status.is_success();
    let reason = status.canonical_reason().unwrap_or("Unknown").to_owned();
    let message = if delivered {
        format!("Push provider accepted the message ({status}).")
    } else if stale {
        format!(
            "Push provider returned {status}. The subscription is stale; reopen the Home Screen app and enable notifications again."
        )
    } else {
        format!("Push provider rejected the message ({status}).")
    };

    Ok(PushDelivery {
        delivered_to_provider: delivered,
        provider_status: status.as_u16(),
        provider_reason: reason,
        stale_subscription: stale,
        message,
    })
}

fn show_status(paths: &Paths) -> Result<()> {
    let vapid_ready = paths.private_key().is_file();
    let subscription = load_subscription(paths).ok();
    println!("VibePing iOS Push PoC status");
    println!("VAPID key:     {}", yes_no(vapid_ready));
    println!("Subscription:  {}", yes_no(subscription.is_some()));
    if let Some(subscription) = subscription {
        println!("Created at:    {}", subscription.created_at.to_rfc3339());
    }
    Ok(())
}

fn reset(paths: &Paths) -> Result<()> {
    let mut removed = 0;
    for path in [paths.private_key(), paths.subscription()] {
        match fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("could not remove {}", path.display()));
            }
        }
    }
    println!("Removed {removed} local state file(s). A new VAPID key will be created on serve.");
    Ok(())
}

fn print_delivery(delivery: &PushDelivery) {
    println!(
        "Provider: {} {}",
        delivery.provider_status, delivery.provider_reason
    );
    println!("{}", delivery.message);
}

fn yes_no(value: bool) -> &'static str {
    if value { "ready" } else { "missing" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(root: &std::path::Path) -> Paths {
        Paths {
            public_dir: root.join("public"),
            data_dir: root.join("data"),
        }
    }

    #[test]
    fn vapid_key_is_reused_across_restarts() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = test_paths(temp.path());
        let first = create_or_load_vapid(&paths).expect("first VAPID key");
        let second = create_or_load_vapid(&paths).expect("reloaded VAPID key");
        assert_eq!(public_key_base64(&first), public_key_base64(&second));
    }

    #[test]
    fn subscription_requires_an_https_endpoint() {
        let subscription = PushSubscription {
            endpoint: "http://example.test/push".to_owned(),
            expiration_time: None,
            keys: SubscriptionKeys {
                p256dh: "not-empty".to_owned(),
                auth: "not-empty".to_owned(),
            },
            created_at: Utc::now(),
        };
        assert_eq!(
            validate_subscription(&subscription),
            Err("push endpoint must use HTTPS".to_owned())
        );
    }

    #[test]
    fn saved_subscription_round_trips_without_exposing_it() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = test_paths(temp.path());
        let subscription = PushSubscription {
            endpoint: "https://push.example.test/id".to_owned(),
            expiration_time: None,
            keys: SubscriptionKeys {
                p256dh: "public-client-key".to_owned(),
                auth: "authentication-secret".to_owned(),
            },
            created_at: Utc::now(),
        };
        save_subscription(&paths, &subscription).expect("save subscription");
        let loaded = load_subscription(&paths).expect("load subscription");
        assert_eq!(loaded.endpoint, subscription.endpoint);
        assert_eq!(loaded.keys.auth, subscription.keys.auth);
    }
}
