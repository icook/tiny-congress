#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tinycongress_api::sim::{client::SimClient, identity::SimAccount};
use tower_http::cors::CorsLayer;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

struct Config {
    api_url: String,
    listen_addr: String,
    allowed_callback: String,
    log_level: String,
}

impl Config {
    fn from_env() -> Result<Self, anyhow::Error> {
        let api_url =
            std::env::var("DEMO_VERIFIER_API_URL").context("DEMO_VERIFIER_API_URL is required")?;
        let listen_addr = std::env::var("DEMO_VERIFIER_LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8090".to_string());
        let allowed_callback = std::env::var("DEMO_VERIFIER_ALLOWED_CALLBACK")
            .context("DEMO_VERIFIER_ALLOWED_CALLBACK is required")?;
        let log_level =
            std::env::var("DEMO_VERIFIER_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        Ok(Self {
            api_url,
            listen_addr,
            allowed_callback,
            log_level,
        })
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct AppState {
    client: SimClient,
    verifier: SimAccount,
    allowed_callback: String,
    ready: AtomicBool,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct VerifyRequest {
    username: String,
    method: String,
    callback: String,
}

#[derive(Serialize)]
struct VerifyResponse {
    redirect: String,
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    if state.ready.load(Ordering::Relaxed) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn index_page() -> Html<String> {
    Html(verification_html())
}

async fn verify(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VerifyRequest>,
) -> impl IntoResponse {
    // Validate callback URL prefix
    if !req.callback.starts_with(&state.allowed_callback) {
        let msg = urlencoding::encode("Invalid callback URL");
        return Json(VerifyResponse {
            redirect: format!(
                "{}/verify/callback?verification=error&message={msg}",
                req.callback
            ),
        });
    }

    // Validate method
    let valid_methods = ["government_id", "phone", "email"];
    if !valid_methods.contains(&req.method.as_str()) {
        let msg = urlencoding::encode("Invalid verification method");
        return Json(VerifyResponse {
            redirect: format!(
                "{}/verify/callback?verification=error&message={msg}",
                req.callback
            ),
        });
    }

    // Build evidence JSON
    let evidence = serde_json::json!({
        "method": req.method,
        "provider": "demo_verifier"
    });

    // Call TC API to create endorsement
    let result = state
        .client
        .endorse_with_evidence(
            &state.verifier,
            &req.username,
            "identity_verified",
            Some(&evidence),
        )
        .await;

    match result {
        Ok(()) => {
            tracing::info!(username = %req.username, method = %req.method, "endorsement created");
            Json(VerifyResponse {
                redirect: format!(
                    "{}/verify/callback?verification=success&method={}",
                    req.callback, req.method
                ),
            })
        }
        Err(e) => {
            let err_msg = e.to_string();
            // 409 means already endorsed — treat as success
            if err_msg.contains("409") {
                tracing::info!(
                    username = %req.username,
                    method = %req.method,
                    "already endorsed (409)"
                );
                Json(VerifyResponse {
                    redirect: format!(
                        "{}/verify/callback?verification=success&method={}",
                        req.callback, req.method
                    ),
                })
            } else {
                tracing::error!(
                    username = %req.username,
                    error = %e,
                    "endorsement failed"
                );
                let error_detail = format!("Verification failed: {e}");
                let msg = urlencoding::encode(&error_detail);
                Json(VerifyResponse {
                    redirect: format!(
                        "{}/verify/callback?verification=error&message={msg}",
                        req.callback
                    ),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTML page
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
fn verification_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Demo Identity Verifier</title>
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
    background: #f0f2f5;
    color: #1a1a2e;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .container {
    background: #ffffff;
    border-radius: 8px;
    box-shadow: 0 2px 12px rgba(0,0,0,0.08);
    max-width: 480px;
    width: 100%;
    padding: 40px 32px;
  }
  .logo-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 32px;
    padding-bottom: 20px;
    border-bottom: 2px solid #e2e6ea;
  }
  .logo-icon {
    width: 40px;
    height: 40px;
    background: #3b5998;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 700;
    font-size: 18px;
  }
  .logo-text {
    font-size: 18px;
    font-weight: 600;
    color: #3b5998;
  }
  h1 {
    font-size: 22px;
    font-weight: 600;
    margin-bottom: 8px;
    color: #1a1a2e;
  }
  .subtitle {
    font-size: 14px;
    color: #6b7280;
    margin-bottom: 28px;
    line-height: 1.5;
  }
  .username-display {
    background: #f8f9fb;
    border: 1px solid #e2e6ea;
    border-radius: 6px;
    padding: 12px 16px;
    margin-bottom: 24px;
    font-size: 14px;
    color: #374151;
  }
  .username-display strong {
    color: #1a1a2e;
  }
  .method-group {
    margin-bottom: 28px;
  }
  .method-group label.group-label {
    display: block;
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: #6b7280;
    margin-bottom: 12px;
  }
  .method-option {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid #e2e6ea;
    border-radius: 6px;
    margin-bottom: 8px;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
  }
  .method-option:hover {
    border-color: #3b5998;
    background: #f8f9fb;
  }
  .method-option.selected {
    border-color: #3b5998;
    background: #eef2ff;
  }
  .method-option input[type="radio"] {
    accent-color: #3b5998;
    width: 18px;
    height: 18px;
    cursor: pointer;
  }
  .method-info {
    flex: 1;
  }
  .method-name {
    font-size: 15px;
    font-weight: 500;
    color: #1a1a2e;
  }
  .method-desc {
    font-size: 12px;
    color: #9ca3af;
    margin-top: 2px;
  }
  .submit-btn {
    width: 100%;
    padding: 14px;
    background: #3b5998;
    color: #fff;
    border: none;
    border-radius: 6px;
    font-size: 15px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }
  .submit-btn:disabled {
    background: #9ca3af;
    cursor: not-allowed;
  }
  .submit-btn:not(:disabled):hover {
    background: #2d4373;
  }
  .spinner {
    display: none;
    text-align: center;
    padding: 20px;
    color: #6b7280;
    font-size: 14px;
  }
  .spinner.active { display: block; }
  .spinner .dot-pulse {
    display: inline-block;
    width: 8px;
    height: 8px;
    background: #3b5998;
    border-radius: 50%;
    animation: pulse 1s ease-in-out infinite;
    margin: 0 3px;
  }
  .spinner .dot-pulse:nth-child(2) { animation-delay: 0.2s; }
  .spinner .dot-pulse:nth-child(3) { animation-delay: 0.4s; }
  @keyframes pulse {
    0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
    40% { opacity: 1; transform: scale(1); }
  }
  .error-box {
    display: none;
    background: #fef2f2;
    border: 1px solid #fecaca;
    border-radius: 6px;
    padding: 12px 16px;
    margin-bottom: 16px;
    font-size: 14px;
    color: #991b1b;
  }
  .error-box.active { display: block; }
  .disclaimer {
    margin-top: 24px;
    padding-top: 20px;
    border-top: 1px solid #e2e6ea;
    font-size: 12px;
    color: #9ca3af;
    text-align: center;
    line-height: 1.5;
  }
  .missing-params {
    text-align: center;
    padding: 20px;
    color: #991b1b;
    font-size: 14px;
  }
</style>
</head>
<body>
  <div class="container">
    <div class="logo-bar">
      <div class="logo-icon">DV</div>
      <div class="logo-text">Demo Identity Verifier</div>
    </div>

    <div id="missing-params" class="missing-params" style="display:none;">
      <p>Missing required parameters. This page should be opened from the TinyCongress app.</p>
    </div>

    <div id="main-content">
      <h1>Identity Verification</h1>
      <p class="subtitle">
        Select a verification method below to confirm your identity.
        This will grant you voting eligibility on TinyCongress.
      </p>

      <div class="username-display">
        Verifying account: <strong id="username-label"></strong>
      </div>

      <div id="error-box" class="error-box"></div>

      <form id="verify-form">
        <div class="method-group">
          <label class="group-label">Verification Method</label>

          <label class="method-option" id="opt-government_id">
            <input type="radio" name="method" value="government_id">
            <div class="method-info">
              <div class="method-name">Government ID</div>
              <div class="method-desc">Verify with a government-issued photo ID</div>
            </div>
          </label>

          <label class="method-option" id="opt-phone">
            <input type="radio" name="method" value="phone">
            <div class="method-info">
              <div class="method-name">Phone Verification</div>
              <div class="method-desc">Verify via SMS code to your phone number</div>
            </div>
          </label>

          <label class="method-option" id="opt-email">
            <input type="radio" name="method" value="email">
            <div class="method-info">
              <div class="method-name">Email Verification</div>
              <div class="method-desc">Verify via a link sent to your email address</div>
            </div>
          </label>
        </div>

        <button type="submit" class="submit-btn" id="submit-btn" disabled>
          Complete Verification
        </button>
      </form>

      <div id="spinner" class="spinner">
        <span class="dot-pulse"></span>
        <span class="dot-pulse"></span>
        <span class="dot-pulse"></span>
        <br><br>
        Processing verification...
      </div>

      <div class="disclaimer">
        This is a demo verifier. In production, real identity providers would be used.
      </div>
    </div>
  </div>

<script>
(function() {
  var params = new URLSearchParams(window.location.search);
  var username = params.get('username');
  var callback = params.get('callback');

  if (!username || !callback) {
    document.getElementById('main-content').style.display = 'none';
    document.getElementById('missing-params').style.display = 'block';
    return;
  }

  document.getElementById('username-label').textContent = username;

  var form = document.getElementById('verify-form');
  var submitBtn = document.getElementById('submit-btn');
  var spinner = document.getElementById('spinner');
  var errorBox = document.getElementById('error-box');
  var radios = form.querySelectorAll('input[name="method"]');
  var options = form.querySelectorAll('.method-option');

  // Enable submit when a method is selected and highlight the option
  radios.forEach(function(radio) {
    radio.addEventListener('change', function() {
      submitBtn.disabled = false;
      options.forEach(function(opt) { opt.classList.remove('selected'); });
      radio.closest('.method-option').classList.add('selected');
    });
  });

  form.addEventListener('submit', function(e) {
    e.preventDefault();
    errorBox.classList.remove('active');

    var selected = form.querySelector('input[name="method"]:checked');
    if (!selected) { return; }

    // Show loading state
    form.style.display = 'none';
    spinner.classList.add('active');

    fetch('/api/verify', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        username: username,
        method: selected.value,
        callback: callback
      })
    })
    .then(function(resp) { return resp.json(); })
    .then(function(data) {
      if (data.redirect) {
        window.location.href = data.redirect;
      } else {
        throw new Error('No redirect URL in response');
      }
    })
    .catch(function(err) {
      spinner.classList.remove('active');
      form.style.display = 'block';
      errorBox.textContent = 'Verification request failed: ' + err.message;
      errorBox.classList.add('active');
    });
  });
})();
</script>
</body>
</html>"#.to_string()
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Load config from DEMO_VERIFIER_* env vars
    let config = Config::from_env().context("failed to load demo_verifier config")?;

    // 2. Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.log_level)
                .map_err(|e| anyhow::anyhow!("invalid log level '{}': {e}", config.log_level))?,
        )
        .init();

    tracing::info!("demo_verifier starting up");
    tracing::info!(
        api_url = %config.api_url,
        listen_addr = %config.listen_addr,
        allowed_callback = %config.allowed_callback,
        "config loaded"
    );

    // 3. Create HTTP client and derive verifier identity
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .context("failed to build HTTP client")?;
    let client = SimClient::new(http, config.api_url.clone());
    let verifier = SimAccount::demo_verifier();

    tracing::info!(
        username = %verifier.username,
        root_pubkey = %verifier.root_pubkey_base64url(),
        "demo verifier identity (ensure TC_VERIFIERS includes this public key)"
    );

    // 4. Build app state and start server — login happens in background
    let state = Arc::new(AppState {
        client,
        verifier,
        allowed_callback: config.allowed_callback,
        ready: AtomicBool::new(false),
    });

    // Spawn background login retry so the server starts immediately.
    // Retries every 5s for up to 5 minutes — plenty of headroom for the API
    // pod to come up during a Helm upgrade.
    let bg_state = Arc::clone(&state);
    tokio::spawn(async move {
        let max_attempts = 60;
        let delay = std::time::Duration::from_secs(5);
        for attempt in 1..=max_attempts {
            let login_body = bg_state.verifier.build_login_json();
            match bg_state.client.login(&login_body).await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if status == 201 {
                        tracing::info!("demo verifier device key registered via login");
                        bg_state.ready.store(true, Ordering::Relaxed);
                    } else if status == 409 {
                        tracing::debug!("demo verifier device key already registered");
                        bg_state.ready.store(true, Ordering::Relaxed);
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        tracing::warn!(
                            status,
                            body = %body,
                            "demo verifier login returned unexpected status"
                        );
                    }
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        attempt,
                        error = %e,
                        "could not reach API for login — retrying in 5s"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
        tracing::error!("demo verifier login failed after {max_attempts} attempts");
    });

    let app = Router::new()
        .route("/", get(index_page))
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/verify", post(verify))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .context("failed to bind listener")?;

    tracing::info!(addr = %config.listen_addr, "demo verifier listening");

    axum::serve(listener, app)
        .await
        .context("axum server error")?;

    Ok(())
}
