/**
 * <vapor-mark> — the Vapor wordmark, generated.
 *
 * RENDERING MODEL — read before changing anything.
 *
 * The mark is a liquid tube. It is not an outline, and it is no longer a
 * stack of stamped strokes. Both of those failed for the same reason: they
 * describe the SILHOUETTE and then try to paint volume onto it, so every
 * cap, join and self-overlap becomes a seam that has to be special-cased.
 *
 * Instead the tube is a SIGNED DISTANCE FIELD, evaluated per pixel:
 *
 *   1. The centreline is a chain of ~64 nodes, each with a radius. Distance
 *      to the chain = min over segments of (distance to segment − radius).
 *   2. Arms that are far apart in arc length but close in space are unioned
 *      with a SMOOTH minimum, so where the W folds back on itself the two
 *      arms fuse with a meniscus fillet instead of a crease. This is the
 *      whole "liquid" read, and it is one line of maths.
 *   3. Height above the page is the tube's circular cross-section,
 *      h = sqrt(r² − dc²). The surface normal is the gradient of h.
 *   4. Lighting is analytic from that normal: diffuse, a tight specular
 *      ridge, a broad sheen, a Fresnel rim, and a transmission caustic on
 *      the flank facing away from the light. Nothing is stamped, so nothing
 *      can disagree with anything else.
 *   5. Alpha is the field itself — smoothstep across one pixel — so the edge
 *      is analytically antialiased and round ends are round because a
 *      distance field has no other option.
 *
 * The field is computed at reduced resolution (shading is smooth, so it
 * upscales for free) with a uniform grid over the segments for culling, and
 * the update rate adapts to the measured cost. Colour, radius and arc length
 * all travel with the nearest arm, so the iridescence flows ALONG the tube
 * rather than across the screen.
 *
 * In Godot: this is a fragment shader almost verbatim — the node chain goes
 * in as a uniform array, sdSegment/smin are the same functions, and the
 * shading block ports line for line.
 *
 * Attributes
 *   size      px (square). Default 160.
 *   theme     "light" | "dark"
 *   state     "idle" | "playing" | "blending" | "thinking"
 *   energy    0..1 — amplitude drive (playing)
 *   speed     motion multiplier, default 1
 *   hue       hue offset in degrees, default 0
 *   static    present = render one frame, no rAF (for print/export)
 */
(function () {
  const TAU = Math.PI * 2;
  const lerp = (a, b, t) => a + (b - a) * t;
  const clamp = (v, a, b) => (v < a ? a : v > b ? b : v);

  // Iridescent ramp along the tube: pale periwinkle body with a violet→aqua
  // flash through the middle, matching the painted mark.
  const RAMP = [
    [0.00, [216, 229, 252]],
    [0.15, [176, 197, 247]],
    [0.30, [150, 170, 241]],
    [0.42, [136, 116, 241]],
    [0.53, [ 96, 206, 222]],
    [0.65, [152, 188, 245]],
    [0.83, [188, 210, 249]],
    [1.00, [214, 228, 252]],
  ];

  function hueMatrix(deg) {
    const a = (deg * Math.PI) / 180, c = Math.cos(a), s = Math.sin(a);
    return [
      0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928,
      0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.140, 0.072 - c * 0.072 - s * 0.283,
      0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072,
    ];
  }

  /** 256-entry colour LUT for the ramp, hue-rotated once per frame. */
  const LUT_N = 256;
  const lutR = new Float32Array(LUT_N), lutG = new Float32Array(LUT_N), lutB = new Float32Array(LUT_N);
  function buildLUT(hueShift) {
    const m = hueShift ? hueMatrix(hueShift) : null;
    let seg = 0;
    for (let i = 0; i < LUT_N; i++) {
      const t = i / (LUT_N - 1);
      while (seg < RAMP.length - 2 && t > RAMP[seg + 1][0]) seg++;
      const [t0, c0] = RAMP[seg], [t1, c1] = RAMP[seg + 1];
      const k = (t - t0) / (t1 - t0 || 1);
      let r = lerp(c0[0], c1[0], k), g = lerp(c0[1], c1[1], k), b = lerp(c0[2], c1[2], k);
      if (m) {
        const nr = r * m[0] + g * m[1] + b * m[2];
        const ng = r * m[3] + g * m[4] + b * m[5];
        const nb = r * m[6] + g * m[7] + b * m[8];
        r = nr; g = ng; b = nb;
      }
      lutR[i] = clamp(r, 0, 255); lutG[i] = clamp(g, 0, 255); lutB[i] = clamp(b, 0, 255);
    }
  }

  // Light: upper-left, tipped towards the viewer. Every shading term derives
  // from this one vector, which is why the volume never contradicts itself.
  const LX = -0.46, LY = -0.70, LZ = 0.55;
  const LL = Math.hypot(LX, LY, LZ);
  const lx = LX / LL, ly = LY / LL, lz = LZ / LL;
  // Blinn half-vector against V = (0,0,1).
  const hxr = lx, hyr = ly, hzr = lz + 1;
  const HL = Math.hypot(hxr, hyr, hzr);
  const hx = hxr / HL, hy = hyr / HL, hz = hzr / HL;

  const NODES = 48;
  // Two points on the tube fuse with a fillet only if they are far apart
  // ALONG the path. Measured in arc length against the tube's own radius — a
  // gate counted in segments inflates a straight arm against itself every few
  // nodes, which shows up as periodic dashes down the rim. The ramp is smooth
  // because a hard gate flickers across the medial line between two arms.
  const SEP_LO = 5.5, SEP_HI = 9.5;   // × maxR
  const smoothstep01 = (x) => (x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x));

  /**
   * The centreline: a chirped, damped cosine — amplitude decays and frequency
   * rises left→right, which is what gives the mark its settling-waveform read.
   * Radius tapers from a fat opening stroke to a small tail bead, with a slow
   * travelling swell so the tube reads as liquid rather than extruded.
   */
  function buildPath(p) {
    const xs = new Float32Array(NODES), ys = new Float32Array(NODES), rs = new Float32Array(NODES);
    for (let i = 0; i < NODES; i++) {
      const t = i / (NODES - 1);
      const x = Math.pow(t, 0.87);
      const theta = TAU * (p.f0 * t + p.chirp * t * t) + p.phase;
      const y = -Math.exp(-p.damping * t) * p.amp * Math.cos(theta);
      // Taper · travelling swell · tail bead. Long lazy swells only: more
      // than a couple of cycles along the path reads as corduroy, not liquid.
      const taper = 0.36 + 0.64 * Math.exp(-1.30 * t);
      const swell = 1 + 0.050 * Math.sin(t * 1.7 - p.flow) + 0.028 * Math.sin(t * 3.1 + p.flow * 0.6);
      const bead = 1 + 0.13 * Math.exp(-Math.pow((t - 1) / 0.13, 2));
      xs[i] = x; ys[i] = y; rs[i] = p.width * taper * swell * bead;
    }
    if (p.tilt) {
      const a = (p.tilt * Math.PI) / 180, ca = Math.cos(a), sa = Math.sin(a);
      for (let i = 0; i < NODES; i++) {
        const qx = xs[i] - 0.5, qy = ys[i];
        xs[i] = qx * ca - qy * sa + 0.5;
        ys[i] = qx * sa + qy * ca;
      }
    }
    // Arc length, normalised — the parameter the iridescence rides on.
    const ss = new Float32Array(NODES);
    for (let i = 1; i < NODES; i++) ss[i] = ss[i - 1] + Math.hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
    const total = ss[NODES - 1] || 1;
    for (let i = 0; i < NODES; i++) ss[i] /= total;

    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (let i = 0; i < NODES; i++) {
      minX = Math.min(minX, xs[i] - rs[i]); maxX = Math.max(maxX, xs[i] + rs[i]);
      minY = Math.min(minY, ys[i] - rs[i]); maxY = Math.max(maxY, ys[i] + rs[i]);
    }
    return { xs, ys, rs, ss, box: { minX, maxX, minY, maxY } };
  }

  class VaporMark extends HTMLElement {
    static get observedAttributes() {
      return ['size', 'theme', 'state', 'energy', 'speed', 'hue', 'static'];
    }

    constructor() {
      super();
      this._root = this.attachShadow({ mode: 'open' });
      this._canvas = document.createElement('canvas');
      this._canvas.style.display = 'block';
      const style = document.createElement('style');
      style.textContent = ':host{display:inline-block;line-height:0}';
      this._root.append(style, this._canvas);
      this._ctx = this._canvas.getContext('2d');
      this._t0 = performance.now();
      this._raf = 0;
      this._visible = true;
      this._energySm = 0.5;
      this._skip = 0;        // frames to coast on the cached field
      this._tick = 0;
      // Scratch, reused every frame.
      this._field = document.createElement('canvas');
      this._fctx = this._field.getContext('2d');
      this._bed = document.createElement('canvas');
      this._bctx = this._bed.getContext('2d');
      this._sd = new Float32Array(NODES);
      this._touch = new Int32Array(NODES);
    }

    connectedCallback() {
      this._resize();
      if ('IntersectionObserver' in window) {
        this._io = new IntersectionObserver((es) => {
          this._visible = es.some((e) => e.isIntersecting);
        }, { rootMargin: '80px' });
        this._io.observe(this);
      }
      this._reduced = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      if (this.hasAttribute('static') || this._reduced) this._draw(0);
      else this._loop();
    }

    disconnectedCallback() {
      cancelAnimationFrame(this._raf);
      if (this._io) this._io.disconnect();
    }

    attributeChangedCallback(name) {
      if (name === 'size') this._resize();
      if (this.hasAttribute('static') || this._reduced) this._draw(0);
    }

    get _size() { return parseFloat(this.getAttribute('size')) || 160; }

    _resize() {
      const s = this._size;
      const dpr = this._dpr = Math.min(window.devicePixelRatio || 1, 2.5);
      this._canvas.width = Math.round(s * dpr);
      this._canvas.height = Math.round(s * dpr);
      this._canvas.style.width = s + 'px';
      this._canvas.style.height = s + 'px';
      // Shading is smooth, so the field can live below device resolution and
      // upscale; the edge stays clean because alpha comes from the field too.
      const F = Math.round(clamp(s * dpr, 48, 224));
      if (this._F !== F || !this._img) {
        this._field.width = this._field.height = F;
        this._bed.width = this._bed.height = F;
        this._img = this._fctx.createImageData(F, F);
        this._H = new Float32Array(F * F);
        this._T = new Float32Array(F * F);
        this._Sf = new Float32Array(F * F);
        this._Af = new Float32Array(F * F);
      }
      this._F = F;
      this._skip = 0;
    }

    _loop() {
      this._raf = requestAnimationFrame(() => this._loop());
      if (!this._visible) return;
      this._draw((performance.now() - this._t0) / 1000);
    }

    /** Per-state motion. This is the whole personality of the mark. */
    _params(time) {
      const state = this.getAttribute('state') || 'idle';
      const speed = parseFloat(this.getAttribute('speed')) || 1;
      const target = clamp(parseFloat(this.getAttribute('energy')) || 0.5, 0, 1);
      this._energySm += (target - this._energySm) * 0.06;
      const e = this._energySm;
      const T = time * speed;

      const base = {
        f0: 1.10, chirp: 1.00, damping: 1.12, tilt: -3, amp: 0.47,
        width: 0.132, phase: 0, hotspot: 0.44, hueShift: 0, flow: T * 0.9,
      };
      const hue = parseFloat(this.getAttribute('hue')) || 0;

      if (state === 'playing') {
        return Object.assign(base, {
          amp: 0.40 + 0.14 * e + Math.sin(T * 1.5) * 0.018,
          width: 0.122 + 0.026 * e,
          chirp: 0.94 + 0.20 * e,
          phase: Math.sin(T * 0.62) * 0.10,
          hotspot: 0.40 + 0.22 * (0.5 + 0.5 * Math.sin(T * 0.42)),
          hueShift: hue + Math.sin(T * 0.24) * 14,
          flow: T * (1.4 + e),
        });
      }
      if (state === 'blending') {
        return Object.assign(base, {
          amp: 0.47 + Math.sin(T * 2.1) * 0.045,
          width: 0.138, f0: 1.10 + Math.sin(T * 0.9) * 0.14, chirp: 1.16,
          phase: T * 0.55, hotspot: (T * 0.34) % 1, hueShift: hue + T * 26, flow: T * 2.2,
        });
      }
      if (state === 'thinking') {
        return Object.assign(base, {
          amp: 0.44 + Math.sin(T * 1.1) * 0.060,
          width: 0.126, chirp: 1.00 + Math.sin(T * 0.75) * 0.26,
          phase: T * 1.15, hotspot: (T * 0.55) % 1, hueShift: hue + T * 42, flow: T * 1.7,
        });
      }
      return Object.assign(base, {                       // idle — a tidal breath
        amp: 0.465 + Math.sin(T * 0.34) * 0.026,
        width: 0.130 + Math.sin(T * 0.29) * 0.006,
        phase: Math.sin(T * 0.21) * 0.07,
        hotspot: 0.42 + 0.16 * Math.sin(T * 0.26),
        hueShift: hue + Math.sin(T * 0.17) * 10,
        flow: T * 0.55,
      });
    }

    _draw(time) {
      const animated = !(this.hasAttribute('static') || this._reduced);
      if (animated && this._skip > 0 && this._tick++ % (this._skip + 1) !== 0) {
        this._composite();
        return;
      }
      const t0 = performance.now();
      this._shade(this._params(time));
      this._composite();
      if (animated) {
        const ms = performance.now() - t0;
        // Coast on the cached field when the shade pass is expensive; the
        // motion is slow enough that 20–30fps of field updates is invisible.
        if (ms > 7 && this._skip < 3) this._skip++;
        else if (ms < 3 && this._skip > 0) this._skip--;
      }
    }

    /** Evaluate the distance field and shade it into this._field. */
    _shade(p) {
      const F = this._F, img = this._img, data = img.data;
      const { xs, ys, rs, ss, box } = buildPath(p);
      buildLUT(p.hueShift);
      const light = (this.getAttribute('theme') || 'light') === 'light';

      // Fit the figure into the field with a little breathing room.
      const pad = F * 0.055;
      const bw = box.maxX - box.minX, bh = box.maxY - box.minY;
      const k = Math.min((F - pad * 2) / bw, (F - pad * 2) / bh);
      const ox = pad - box.minX * k + (F - pad * 2 - bw * k) / 2;
      const oy = pad - box.minY * k + (F - pad * 2 - bh * k) / 2;

      // Node chain in field pixels.
      const px = new Float32Array(NODES), py = new Float32Array(NODES), pr = new Float32Array(NODES);
      let maxR = 0, arcPx = 0;
      for (let i = 0; i < NODES; i++) {
        px[i] = xs[i] * k + ox; py[i] = ys[i] * k + oy; pr[i] = rs[i] * k;
        if (pr[i] > maxR) maxR = pr[i];
        if (i) arcPx += Math.hypot(px[i] - px[i - 1], py[i] - py[i - 1]);
      }
      const K = maxR * 0.70;              // fillet radius of the smooth union
      const NS = NODES - 1;
      const sepLo = SEP_LO * maxR, sepSpan = (SEP_HI - SEP_LO) * maxR;

      // Uniform grid over the segments, so each pixel only tests its
      // neighbours. Counting-sorted into one contiguous array — chasing a
      // linked list costs more than the distance maths it saves.
      const cell = Math.max(6, Math.ceil(maxR * 0.9));
      const gw = Math.ceil(F / cell), gh = gw, nCell = gw * gh;
      if (!this._cnt || this._cnt.length < nCell + 1) this._cnt = new Int32Array(nCell + 1);
      const cnt = this._cnt; cnt.fill(0, 0, nCell + 1);
      const rmax = new Float32Array(NS);
      const cx0 = new Int32Array(NS), cx1 = new Int32Array(NS);
      const cy0 = new Int32Array(NS), cy1 = new Int32Array(NS);
      let total = 0;
      for (let i = 0; i < NS; i++) {
        rmax[i] = Math.max(pr[i], pr[i + 1]);
        const reach = rmax[i] + K + 1;
        cx0[i] = Math.max(0, Math.floor((Math.min(px[i], px[i + 1]) - reach) / cell));
        cx1[i] = Math.min(gw - 1, Math.floor((Math.max(px[i], px[i + 1]) + reach) / cell));
        cy0[i] = Math.max(0, Math.floor((Math.min(py[i], py[i + 1]) - reach) / cell));
        cy1[i] = Math.min(gh - 1, Math.floor((Math.max(py[i], py[i + 1]) + reach) / cell));
        for (let gy = cy0[i]; gy <= cy1[i]; gy++) {
          for (let gx = cx0[i]; gx <= cx1[i]; gx++) { cnt[gy * gw + gx + 1]++; total++; }
        }
      }
      for (let c = 0; c < nCell; c++) cnt[c + 1] += cnt[c];
      if (!this._items || this._items.length < total) this._items = new Int32Array(total * 2);
      const items = this._items;
      if (!this._cur || this._cur.length < nCell) this._cur = new Int32Array(nCell);
      const cur = this._cur;
      for (let c = 0; c < nCell; c++) cur[c] = cnt[c];
      for (let i = 0; i < NS; i++) {
        for (let gy = cy0[i]; gy <= cy1[i]; gy++) {
          for (let gx = cx0[i]; gx <= cx1[i]; gx++) items[cur[gy * gw + gx]++] = i;
        }
      }
      // Which cells can actually fuse two arms? Only cells holding segments
      // far apart along the path. Everywhere else — which is nearly
      // everywhere — the pixel loop skips the fillet search entirely and
      // rejects distant segments on a much tighter bound.
      if (!this._fuse || this._fuse.length < nCell) this._fuse = new Uint8Array(nCell);
      const fuse = this._fuse;
      for (let c = 0; c < nCell; c++) {
        const a = cnt[c], e = cnt[c + 1];
        if (e - a < 2) { fuse[c] = 0; continue; }
        let lo = items[a], hi = items[a];
        for (let l = a + 1; l < e; l++) {
          const v = items[l];
          if (v < lo) lo = v; else if (v > hi) hi = v;
        }
        fuse[c] = (ss[hi] - ss[lo]) * arcPx >= sepLo ? 1 : 0;
      }

      const sd = this._sd, touch = this._touch;
      const px1 = 1.05;                       // edge softness, in field pixels
      let idx = 0;

      // Two passes: build the field, then shade it. The tube's height is
      // analytic, so the normal is just the gradient of that height.
      const H = this._H, T = this._T, Sf = this._Sf, Af = this._Af;
      Af.fill(0);

      for (let y = 0; y < F; y++) {
        const gyBase = Math.min(gh - 1, (y / cell) | 0) * gw;
        for (let x = 0; x < F; x++, idx++) {
          const c = gyBase + Math.min(gw - 1, (x / cell) | 0);
          const cEnd = cnt[c + 1];
          const cStart = cnt[c];
          if (cStart >= cEnd) continue;
          const fusing = fuse[c];
          const slack = fusing ? K : 0;
          let best = 1e9, bestSeg = -1, bestT = 0, nt = 0;

          for (let l = cStart; l < cEnd; l++) {
            const i = items[l];
            const ax = px[i], ay = py[i];
            const bx = px[i + 1] - ax, by = py[i + 1] - ay;
            const wx = x + 0.5 - ax, wy = y + 0.5 - ay;
            const l2 = bx * bx + by * by;
            let t = l2 > 0 ? (wx * bx + wy * by) / l2 : 0;
            t = t < 0 ? 0 : t > 1 ? 1 : t;
            const dx = wx - bx * t, dy = wy - by * t;
            const dd = dx * dx + dy * dy;
            // Reject on the squared distance before paying for a sqrt: a
            // segment this far out can neither win nor contribute a fillet.
            const lim = best + rmax[i] + slack;
            if (lim > 0 && dd > lim * lim) continue;
            const d = Math.sqrt(dd) - (pr[i] + (pr[i + 1] - pr[i]) * t);
            if (d < best) { best = d; bestSeg = i; bestT = t; }
            if (fusing) { sd[i] = d; touch[nt++] = i; }
          }
          if (best > px1) continue;
          // Smooth union against the nearest OTHER arm — this is the fillet
          // that turns a crossing into a fused pool of liquid. The strength of
          // each candidate's fillet ramps with how far away it is along the
          // path, so nothing switches on abruptly. Most pixels only ever see
          // one arm, so the whole search is skipped unless this cell's
          // segments actually span enough of the path to fuse.
          let d = best;
          if (fusing) {
            for (let m = 0; m < nt; m++) {
              const i = touch[m];
              const kk = K * smoothstep01((Math.abs(ss[i] - ss[bestSeg]) * arcPx - sepLo) / sepSpan);
              if (kk <= 0) continue;
              const di = sd[i];
              if (di - best >= kk) continue;
              const hmix = 0.5 + 0.5 * (di - best) / kk;
              const v = lerp(di, best, hmix) - kk * hmix * (1 - hmix);
              if (v < d) d = v;
            }
          }

          const r = pr[bestSeg] + (pr[bestSeg + 1] - pr[bestSeg]) * bestT;
          const s = ss[bestSeg] + (ss[bestSeg + 1] - ss[bestSeg]) * bestT;
          const dc = d + r > 0 ? d + r : 0;
          const h = Math.sqrt(Math.max(0, r * r - dc * dc));
          H[idx] = h;
          Sf[idx] = s;
          Af[idx] = clamp(0.5 - d / px1, 0, 1);
        }
      }

      // The centreline is piecewise linear, so the dome ridge kinks at every
      // node and the kinks show as faint hatching once the specular exponent
      // amplifies them. One 1–2–1 pass over the height removes them. Taps that
      // fall outside the shape are replaced by the centre value, so the rim
      // keeps its full slope instead of being dragged towards zero.
      for (let y = 0; y < F; y++) {
        for (let x = 0; x < F; x++) {
          const i = y * F + x;
          if (Af[i] <= 0) { T[i] = 0; continue; }
          const c = H[i];
          const a = x > 0 && Af[i - 1] > 0 ? H[i - 1] : c;
          const b = x < F - 1 && Af[i + 1] > 0 ? H[i + 1] : c;
          T[i] = (a + c * 2 + b) * 0.25;
        }
      }
      for (let y = 0; y < F; y++) {
        for (let x = 0; x < F; x++) {
          const i = y * F + x;
          if (Af[i] <= 0) { H[i] = 0; continue; }
          const c = T[i];
          const a = y > 0 && Af[i - F] > 0 ? T[i - F] : c;
          const b = y < F - 1 && Af[i + F] > 0 ? T[i + F] : c;
          H[i] = (a + c * 2 + b) * 0.25;
        }
      }

      // ---- Shading pass: normal from ∇h, then analytic glass -------------
      const hotspot = clamp(p.hotspot, 0, 1);
      const bodyMul = light ? 1 : 0.92;
      for (let y = 0; y < F; y++) {
        for (let x = 0; x < F; x++) {
          const i = y * F + x;
          const a = Af[i];
          const o = i * 4;
          if (a <= 0.002) { data[o + 3] = 0; continue; }

          const xm = x > 0 ? H[i - 1] : 0, xp = x < F - 1 ? H[i + 1] : 0;
          const ym = y > 0 ? H[i - F] : 0, yp = y < F - 1 ? H[i + F] : 0;
          let nx = (xm - xp) * 0.5, ny = (ym - yp) * 0.5, nz = 1;
          const nl = Math.sqrt(nx * nx + ny * ny + 1) || 1;
          nx /= nl; ny /= nl; nz /= nl;

          const ndl = nx * lx + ny * ly + nz * lz;
          const ndh = nx * hx + ny * hy + nz * hz;
          const diff = ndl > 0 ? ndl : 0;
          // Integer power chains — Math.pow in a 90k-pixel loop is the single
          // most expensive thing in the shader.
          let spec = 0, sheen = 0;
          if (ndh > 0) {
            const p2 = ndh * ndh, p4 = p2 * p2, p8 = p4 * p4, p16 = p8 * p8, p32 = p16 * p16;
            spec = p32 * p8 * p4 * p2;          // ^46, the tight ridge
            sheen = p4 * p2;                    // ^6, the broad wet sheen
          }
          const f1 = 1 - nz;
          const f2 = f1 * f1;
          const fres = f2 * f1;
          // Light that has gone through the tube and comes out the far flank.
          const trans = ndl < 0 ? ndl * ndl * f2 : 0;

          // Body colour rides arc length, with the iridescent hotspot on top.
          const sv = Sf[i];
          const li = sv * (LUT_N - 1);
          const lo = li | 0, lf = li - lo, hi2 = lo < LUT_N - 1 ? lo + 1 : lo;
          let r = lerp(lutR[lo], lutR[hi2], lf);
          let g = lerp(lutG[lo], lutG[hi2], lf);
          let b = lerp(lutB[lo], lutB[hi2], lf);
          const hd = (sv - hotspot) * 6.6667;
          if (hd * hd < 9) {
            const hs = Math.exp(-hd * hd);
            r = lerp(r, r * 0.86 + 30, hs * 0.5);
            g = lerp(g, g * 0.94 + 46, hs * 0.5);
            b = lerp(b, b * 0.98 + 18, hs * 0.5);
          }

          // Volume: the deep flank sinks towards a cool shadow, the lit flank
          // rises. Because both come off the same normal they cannot fight.
          const shade = 0.52 + 0.62 * diff;
          r = r * shade * bodyMul;
          g = g * shade * bodyMul;
          b = b * shade * bodyMul;
          // Cool the shadowed side rather than just darkening it.
          const shadow = clamp(-ndl, 0, 1);
          r -= shadow * 34; g -= shadow * 20; b -= shadow * 2;

          const add = spec * 236 + sheen * 34 + fres * (light ? 96 : 122) + trans * 118;
          r += add + trans * 8;
          g += add + trans * 16;
          b += add + trans * 26;

          data[o] = r < 0 ? 0 : r > 255 ? 255 : r;
          data[o + 1] = g < 0 ? 0 : g > 255 ? 255 : g;
          data[o + 2] = b < 0 ? 0 : b > 255 ? 255 : b;
          data[o + 3] = a * 255;
        }
      }
      this._fctx.putImageData(img, 0, 0);
      this._light = light;
    }

    /** Draw the cached field: cast shadow / bloom, then the tube itself. */
    _composite() {
      const ctx = this._ctx, S = this._size, F = this._F, light = this._light !== false;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, this._canvas.width, this._canvas.height);
      ctx.setTransform(this._dpr, 0, 0, this._dpr, 0, 0);
      ctx.imageSmoothingEnabled = true;
      if ('imageSmoothingQuality' in ctx) ctx.imageSmoothingQuality = 'high';

      // Bed: the silhouette, flat-tinted, blurred beneath the mark.
      const b = this._bctx;
      b.setTransform(1, 0, 0, 1, 0, 0);
      b.clearRect(0, 0, F, F);
      b.globalCompositeOperation = 'source-over';
      b.drawImage(this._field, 0, 0);
      b.globalCompositeOperation = 'source-in';
      const bg = b.createLinearGradient(0, 0, F, 0);
      if (light) {
        bg.addColorStop(0, 'rgb(98,126,198)');
        bg.addColorStop(0.5, 'rgb(80,150,192)');
        bg.addColorStop(1, 'rgb(122,142,202)');
      } else {
        bg.addColorStop(0, 'rgb(122,152,255)');
        bg.addColorStop(0.5, 'rgb(142,112,250)');
        bg.addColorStop(1, 'rgb(72,222,206)');
      }
      b.fillStyle = bg;
      b.fillRect(0, 0, F, F);
      b.globalCompositeOperation = 'source-over';

      const canBlur = typeof ctx.filter === 'string';
      ctx.save();
      if (canBlur) ctx.filter = `blur(${S * (light ? 0.045 : 0.06)}px)`;
      ctx.globalAlpha = light ? 0.36 : 0.46;
      ctx.drawImage(this._bed, 0, light ? S * 0.03 : 0, S, S);
      ctx.restore();

      ctx.drawImage(this._field, 0, 0, S, S);
    }
  }

  if (!customElements.get('vapor-mark')) customElements.define('vapor-mark', VaporMark);
})();
