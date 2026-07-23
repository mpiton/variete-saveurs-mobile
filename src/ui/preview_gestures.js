// Pinch-zoom, pan and double-tap for the full-screen document preview
// (task 18). Installed once per viewport by ui/preview.rs via
// `document::eval`. The frame has `pointer-events: none` (app.css) — the
// rendered document holds no interactive content — so every gesture lands
// on the viewport in a single document and a single coordinate space. The
// frame is auto-heighted to its content (same-origin srcdoc), so pan/zoom
// is one CSS transform on `.preview-stage` covering paper and margins.
// Initial view is fit-to-width; double-tap toggles fit-to-width and 100 %.
(() => {
  const viewport = document.getElementById("preview-viewport");
  const stage = document.getElementById("preview-stage");
  const frame = document.getElementById("preview-frame");
  if (!viewport || !stage || !frame || viewport.dataset.previewGestures === "on") {
    return;
  }
  viewport.dataset.previewGestures = "on";

  const MIN_SCALE_RATIO = 0.6; // relative to fit-to-width
  const MAX_SCALE = 3;
  const DOUBLE_TAP_MS = 300;
  const MAX_TAP_MS = 300;
  const TAP_SLOP_PX = 24;

  let scale = 1;
  let tx = 0;
  let ty = 0;
  let contentWidth = 0; // unscaled frame size, px (210 mm ≈ 794)
  let contentHeight = 0;
  let userZoomed = false;
  let viewportRect = viewport.getBoundingClientRect();
  let pinch = null;
  let pinched = false; // the current touch sequence included a pinch → no tap
  let loadedOnce = false;
  let lastTap = { time: 0, x: 0, y: 0 };
  const active = new Map(); // touch identifier → latest viewport-space point
  const touchDown = new Map(); // touch identifier → {x, y, time} at touchdown

  const fitScale = () => (contentWidth > 0 ? viewport.clientWidth / contentWidth : 1);
  const clampScale = (value) =>
    Math.min(MAX_SCALE, Math.max(fitScale() * MIN_SCALE_RATIO, value));

  function apply() {
    const scaledW = contentWidth * scale;
    const scaledH = contentHeight * scale;
    const viewW = viewport.clientWidth;
    const viewH = viewport.clientHeight;
    // Content smaller than the viewport stays centered; otherwise the pan is
    // clamped so no gap shows past the paper edges.
    tx = scaledW <= viewW ? (viewW - scaledW) / 2 : Math.min(0, Math.max(viewW - scaledW, tx));
    ty = scaledH <= viewH ? (viewH - scaledH) / 2 : Math.min(0, Math.max(viewH - scaledH, ty));
    stage.style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
  }

  function fitToWidth() {
    if (contentWidth === 0) {
      return;
    }
    scale = fitScale();
    tx = (viewport.clientWidth - contentWidth * scale) / 2;
    ty = 0;
    userZoomed = false;
    apply();
  }

  function zoomTo(nextScale, cx, cy) {
    if (scale <= 0) {
      return;
    }
    const k = clampScale(nextScale) / scale;
    tx = cx - (cx - tx) * k;
    ty = cy - (cy - ty) * k;
    scale *= k;
    userZoomed = Math.abs(scale - fitScale()) >= 0.02;
    apply();
  }

  // The frame is auto-heighted to its content: panning the stage replaces
  // native scrolling entirely, so one gesture model covers every surface.
  function measure() {
    const doc = frame.contentDocument;
    if (!doc) {
      return false;
    }
    contentWidth = frame.offsetWidth;
    const height = Math.max(
      doc.body ? doc.body.scrollHeight : 0,
      doc.documentElement.scrollHeight,
    );
    if (height > 0) {
      contentHeight = height;
      frame.style.height = `${height}px`;
    }
    return contentWidth > 0 && contentHeight > 0;
  }

  const pointOf = (touch) => ({
    x: touch.clientX - viewportRect.left,
    y: touch.clientY - viewportRect.top,
  });

  function anchorPinch() {
    const [a, b] = [...active.values()];
    pinch = {
      midX: (a.x + b.x) / 2,
      midY: (a.y + b.y) / 2,
      distance: Math.hypot(a.x - b.x, a.y - b.y),
      scale,
      tx,
      ty,
    };
  }

  function onTouchStart(event) {
    event.preventDefault();
    if (active.size === 0) {
      // New touch sequence: refresh the cached rect and re-arm tap detection.
      viewportRect = viewport.getBoundingClientRect();
      pinched = false;
    }
    for (const touch of event.changedTouches) {
      const point = pointOf(touch);
      active.set(touch.identifier, point);
      touchDown.set(touch.identifier, { x: point.x, y: point.y, time: Date.now() });
    }
    if (active.size === 2) {
      anchorPinch();
      pinched = true;
    }
  }

  function onTouchMove(event) {
    event.preventDefault();
    if (active.size === 2) {
      // Re-anchor when the pair formed without a fresh touchstart (a third
      // finger just lifted, leaving two).
      if (!pinch) {
        anchorPinch();
        pinched = true;
      }
      if (pinch.scale <= 0) {
        return;
      }
      for (const touch of event.changedTouches) {
        if (active.has(touch.identifier)) {
          active.set(touch.identifier, pointOf(touch));
        }
      }
      const [a, b] = [...active.values()];
      const midX = (a.x + b.x) / 2;
      const midY = (a.y + b.y) / 2;
      const distance = Math.hypot(a.x - b.x, a.y - b.y);
      // Keep the content point that was under the initial midpoint under the
      // current one: zoom and pan in a single gesture.
      const cX = (pinch.midX - pinch.tx) / pinch.scale;
      const cY = (pinch.midY - pinch.ty) / pinch.scale;
      scale = clampScale((pinch.scale * distance) / Math.max(1, pinch.distance));
      tx = midX - cX * scale;
      ty = midY - cY * scale;
      userZoomed = true;
      apply();
    } else if (active.size === 1) {
      const touch = event.changedTouches[0];
      const previous = active.get(touch.identifier);
      if (!previous) {
        return;
      }
      const point = pointOf(touch);
      tx += point.x - previous.x;
      ty += point.y - previous.y;
      active.set(touch.identifier, point);
      apply();
    }
  }

  function onTouchEnd(event) {
    const now = Date.now();
    for (const touch of event.changedTouches) {
      const down = touchDown.get(touch.identifier);
      active.delete(touch.identifier);
      touchDown.delete(touch.identifier);
      if (active.size < 2) {
        pinch = null;
      }
      if (!down || active.size > 0) {
        continue;
      }
      // Last finger up. A tap is short, stationary since touchdown, and not
      // part of a pinch; two taps close in time and space make a double-tap
      // — zoom adjusted to width, and back to 100 % from there.
      const end = pointOf(touch);
      const isTap =
        !pinched &&
        now - down.time < MAX_TAP_MS &&
        Math.hypot(end.x - down.x, end.y - down.y) < TAP_SLOP_PX;
      const isDoubleTap =
        isTap &&
        now - lastTap.time < DOUBLE_TAP_MS &&
        Math.hypot(end.x - lastTap.x, end.y - lastTap.y) < TAP_SLOP_PX * 2;
      if (isDoubleTap) {
        if (Math.abs(scale - fitScale()) < 0.02) {
          zoomTo(1, end.x, end.y);
        } else {
          fitToWidth();
        }
        lastTap = { time: 0, x: 0, y: 0 };
      } else {
        lastTap = isTap ? { time: now, x: end.x, y: end.y } : { time: 0, x: 0, y: 0 };
      }
    }
  }

  function onFrameLoad() {
    // A reload strands any in-flight gesture and means a new document: reset
    // the state and go back to fit-to-width.
    active.clear();
    touchDown.clear();
    pinch = null;
    if (loadedOnce) {
      userZoomed = false;
    }
    loadedOnce = true;
    if (measure() && !userZoomed) {
      fitToWidth();
    } else {
      apply();
    }
  }

  viewport.addEventListener("touchstart", onTouchStart, { passive: false });
  viewport.addEventListener("touchmove", onTouchMove, { passive: false });
  viewport.addEventListener("touchend", onTouchEnd, { passive: false });
  viewport.addEventListener("touchcancel", onTouchEnd, { passive: false });
  if (frame.contentDocument && frame.contentDocument.readyState === "complete") {
    onFrameLoad();
  }
  frame.addEventListener("load", onFrameLoad);
  window.addEventListener("resize", function onResize() {
    // Self-cleaning: outlives neither the viewport nor the screen visit.
    if (!viewport.isConnected) {
      window.removeEventListener("resize", onResize);
      return;
    }
    viewportRect = viewport.getBoundingClientRect();
    if (!userZoomed) {
      fitToWidth();
    } else {
      apply();
    }
  });
})();
