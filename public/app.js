const elements = {
  https: document.querySelector("#https-status"),
  install: document.querySelector("#install-status"),
  worker: document.querySelector("#worker-status"),
  push: document.querySelector("#push-status"),
  permission: document.querySelector("#permission-status"),
  subscription: document.querySelector("#subscription-status"),
  server: document.querySelector("#server-status"),
  enable: document.querySelector("#enable-button"),
  send: document.querySelector("#send-button"),
  message: document.querySelector("#message"),
};

let workerRegistration;
let serverConnected = false;

function setStatus(element, text, state = "neutral") {
  element.textContent = text;
  element.dataset.state = state;
}

function setMessage(text, state = "neutral") {
  elements.message.textContent = text;
  elements.message.dataset.state = state;
}

function isStandalone() {
  return window.matchMedia("(display-mode: standalone)").matches || window.navigator.standalone === true;
}

function hasPushSupport() {
  return "serviceWorker" in navigator && "PushManager" in window && "Notification" in window;
}

function refreshPermission() {
  if (!("Notification" in window)) {
    setStatus(elements.permission, "Unsupported", "error");
    return;
  }

  const labels = {
    default: ["Not requested", "neutral"],
    granted: ["Granted", "ready"],
    denied: ["Denied", "error"],
  };
  const [label, state] = labels[Notification.permission] || [Notification.permission, "neutral"];
  setStatus(elements.permission, label, state);
}

function updateActions(subscriptionActive = false) {
  const ready = window.isSecureContext && hasPushSupport() && workerRegistration && serverConnected;
  elements.enable.disabled = !ready || Notification.permission === "denied";
  elements.send.hidden = !subscriptionActive;
  elements.send.disabled = !subscriptionActive || !serverConnected;
  elements.enable.textContent = subscriptionActive ? "Refresh subscription" : "Enable notifications";
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, {
    cache: "no-store",
    ...options,
    headers: {
      Accept: "application/json",
      ...(options.body ? { "Content-Type": "application/json" } : {}),
      ...options.headers,
    },
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.error || payload.message || `Request failed (${response.status})`);
  }
  return payload;
}

function base64UrlToUint8Array(value) {
  const padding = "=".repeat((4 - (value.length % 4)) % 4);
  const base64 = (value + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = window.atob(base64);
  return Uint8Array.from(raw, (character) => character.charCodeAt(0));
}

async function saveSubscription(subscription) {
  await fetchJson("/api/subscription", {
    method: "POST",
    body: JSON.stringify(subscription.toJSON()),
  });
}

async function inspectSubscription() {
  if (!workerRegistration || !hasPushSupport()) {
    return false;
  }
  const subscription = await workerRegistration.pushManager.getSubscription();
  if (!subscription) {
    setStatus(elements.subscription, "Not subscribed", "neutral");
    updateActions(false);
    return false;
  }

  setStatus(elements.subscription, "Active", "ready");
  updateActions(true);
  return true;
}

async function initialize() {
  setStatus(elements.https, window.isSecureContext ? "Ready" : "HTTPS required", window.isSecureContext ? "ready" : "error");
  setStatus(elements.install, isStandalone() ? "Installed app" : "Open in browser", isStandalone() ? "ready" : "warning");
  setStatus(elements.push, hasPushSupport() ? "Supported" : "Unsupported", hasPushSupport() ? "ready" : "error");
  refreshPermission();

  try {
    await fetchJson("/api/health");
    serverConnected = true;
    setStatus(elements.server, "Connected", "ready");
  } catch (error) {
    setStatus(elements.server, "Unreachable", "error");
    setMessage(error.message, "error");
  }

  if (!("serviceWorker" in navigator)) {
    setStatus(elements.worker, "Unsupported", "error");
    setMessage("This browser does not support Service Workers.", "error");
    updateActions(false);
    return;
  }

  try {
    workerRegistration = await navigator.serviceWorker.register("/sw.js", {
      scope: "/",
      updateViaCache: "none",
    });
    workerRegistration = await navigator.serviceWorker.ready;
    setStatus(elements.worker, "Ready", "ready");
    const subscribed = await inspectSubscription();
    if (subscribed) {
      setMessage("Subscription is active. The Windows sender can now deliver a test push.", "success");
    } else if (!isStandalone()) {
      setMessage("On iPhone, add this page to the Home Screen and launch it from the icon first.");
    } else {
      setMessage("Ready. Tap Enable notifications when you want to subscribe.");
    }
  } catch (error) {
    setStatus(elements.worker, "Registration failed", "error");
    setMessage(`Service Worker error: ${error.message}`, "error");
  }
  updateActions(await inspectSubscription().catch(() => false));
}

async function enableNotifications() {
  elements.enable.disabled = true;
  setMessage("Preparing notification permission…");

  try {
    workerRegistration ||= await navigator.serviceWorker.ready;
    const permission = await Notification.requestPermission();
    refreshPermission();
    if (permission !== "granted") {
      setMessage(
        permission === "denied"
          ? "Notification permission was denied. Change it in iPhone Settings before retrying."
          : "Notification permission was not granted.",
        permission === "denied" ? "error" : "neutral",
      );
      updateActions(false);
      return;
    }

    const { publicKey } = await fetchJson("/api/vapid-public-key");
    let subscription = await workerRegistration.pushManager.getSubscription();
    if (!subscription) {
      subscription = await workerRegistration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: base64UrlToUint8Array(publicKey),
      });
    }
    await saveSubscription(subscription);
    setStatus(elements.subscription, "Active", "ready");
    setMessage("Subscription active. Lock the iPhone before sending the acceptance-test push.", "success");
    updateActions(true);
  } catch (error) {
    setStatus(elements.subscription, "Failed", "error");
    setMessage(`Could not subscribe: ${error.message}`, "error");
    updateActions(false);
  }
}

async function sendTestNotification() {
  elements.send.disabled = true;
  setMessage("Sending encrypted Web Push…");
  try {
    const result = await fetchJson("/api/test-push", {
      method: "POST",
      body: "{}",
    });
    setMessage(`${result.message} Waiting for the device notification.`, "success");
  } catch (error) {
    setMessage(`Push failed: ${error.message}`, "error");
  } finally {
    elements.send.disabled = false;
  }
}

elements.enable.addEventListener("click", enableNotifications);
elements.send.addEventListener("click", sendTestNotification);
window.addEventListener("pageshow", () => {
  refreshPermission();
  inspectSubscription().catch(() => {});
});

initialize();
