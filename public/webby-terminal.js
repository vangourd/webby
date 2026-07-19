// Loads xterm.js on demand and mounts a terminal wired to /ws/terminal/:runnerId.
// Exposes window.webbyMountTerminal(container, runnerId) -> Promise<cleanupFn>.
(function () {
  const XTERM_CSS = "/vendor/xterm.css";
  const XTERM_JS = "/vendor/xterm.js";
  const FIT_JS = "/vendor/xterm-addon-fit.js";

  function loadScript(src) {
    return new Promise((resolve, reject) => {
      const existing = document.querySelector(`script[src="${src}"]`);
      if (existing) {
        if (existing.dataset.loaded === "true") return resolve();
        existing.addEventListener("load", () => resolve());
        existing.addEventListener("error", reject);
        return;
      }
      const s = document.createElement("script");
      s.src = src;
      s.addEventListener("load", () => {
        s.dataset.loaded = "true";
        resolve();
      });
      s.addEventListener("error", reject);
      document.head.appendChild(s);
    });
  }

  function loadCss(href) {
    return new Promise((resolve, reject) => {
      if (document.querySelector(`link[href="${href}"]`)) return resolve();
      const l = document.createElement("link");
      l.rel = "stylesheet";
      l.href = href;
      l.addEventListener("load", () => resolve());
      l.addEventListener("error", reject);
      document.head.appendChild(l);
    });
  }

  let ready = null;
  function ensureLoaded() {
    if (ready) return ready;
    ready = Promise.all([
      loadCss(XTERM_CSS),
      loadScript(XTERM_JS).then(() => loadScript(FIT_JS)),
    ]);
    return ready;
  }

  window.webbyMountTerminal = async function (container, runnerId) {
    await ensureLoaded();

    const term = new window.Terminal({
      fontFamily: '"Courier Prime", ui-monospace, monospace',
      fontSize: 13,
      cursorBlink: true,
      theme: { background: "#000000" },
      allowProposedApi: true,
      scrollback: 5000,
    });
    const fit = new window.FitAddon.FitAddon();
    term.loadAddon(fit);
    term.open(container);
    try {
      fit.fit();
    } catch (_) {}

    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(
      `${proto}//${location.host}/ws/terminal/${encodeURIComponent(runnerId)}`,
    );
    ws.binaryType = "arraybuffer";

    const encoder = new TextEncoder();

    function sendResize() {
      if (ws.readyState !== WebSocket.OPEN) return;
      ws.send(
        JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }),
      );
    }

    ws.addEventListener("open", () => {
      try {
        fit.fit();
      } catch (_) {}
      sendResize();
    });

    ws.addEventListener("message", (evt) => {
      if (typeof evt.data === "string") {
        try {
          const msg = JSON.parse(evt.data);
          if (msg.type === "runner_disconnected") {
            term.write("\r\n\x1b[31m[runner disconnected]\x1b[0m\r\n");
          }
        } catch (_) {}
      } else {
        term.write(new Uint8Array(evt.data));
      }
    });

    ws.addEventListener("close", () => {
      term.write("\r\n\x1b[33m[connection closed]\x1b[0m\r\n");
    });

    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(encoder.encode(data));
      }
    });

    const resizeObserver = new ResizeObserver(() => {
      try {
        fit.fit();
        sendResize();
      } catch (_) {}
    });
    resizeObserver.observe(container);

    // Focus after next frame so the container is laid out.
    requestAnimationFrame(() => term.focus());

    return function cleanup() {
      try {
        resizeObserver.disconnect();
      } catch (_) {}
      try {
        ws.close();
      } catch (_) {}
      try {
        term.dispose();
      } catch (_) {}
    };
  };
})();
