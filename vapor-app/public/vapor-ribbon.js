/**
 * <vapor-ribbon> — the Vapor mark as a loose twisting ribbon of glass.
 *
 * WHY THIS EXISTS (and why it is not vapor-mark.js)
 *
 * vapor-mark models a TUBE: circular cross-section, height h = sqrt(r²−dc²),
 * normal from ∇h. Two consequences are unavoidable in that model and both are
 * exactly what reads as "worm":
 *   · the highlight is a pill running down the spine, so the eye reads a
 *     sausage regardless of colour;
 *   · the normal is a gradient of a height field built on a piecewise-linear
 *     centreline, so every node kink survives into the specular term as a
 *     faint transverse line. Those are the "individual cylinders".
 *
 * A ribbon needs a different primitive, and it happens to be a cheaper one.
 *
 *   1. The centreline is a smooth analytic curve, sampled at 96 nodes. The
 *      tangent is a central difference of the CLOSED FORM, not of the node
 *      list, so it is smooth by construction.
 *   2. Cross-section is a flat strip of half-width W and half-thickness Th,
 *      rotated about the tangent by a twist angle φ(t) that winds ~1.6 turns
 *      along the path. Its screen half-width is
 *          hw = √((W·cosφ)² + (Th·sinφ)²)
 *      so the ribbon swells broad-side-on and narrows to a bright filament
 *      edge-on. That breathing width IS the twist; nothing else sells it.
 *   3. The surface normal does NOT come from a height field. It is
 *          N = n̂⊥·sin(φ + curl·u) + ẑ·cos(φ + curl·u)
 *      where n̂⊥ is perpendicular to the tangent in screen space and u is the
 *      signed across-width coordinate. Constant along the width except for a
 *      gentle `curl`, so the highlight is a BAND ACROSS the ribbon that
 *      travels as the twist rotates — the opposite of a spine pill. And
 *      because N depends only on smooth analytic quantities, node kinks cannot
 *      print into the shading. That is the striping fix.
 *   4. Glass, not liquid: Schlick Fresnel off |N·V|, Beer absorption over a
 *      path length that grows as the ribbon turns edge-on, chromatically split
 *      refraction of a small environment texture, a tight glint and a broad
 *      sheen, and a bright rim where the silhouette is.
 *   5. Self-crossings are NOT fused. A ribbon overlaps itself; welding it with
 *      a smooth minimum is what made the old mark look like poured wax. The
 *      path carries a z coordinate, each pixel keeps its two nearest arms, and
 *      the nearer one is composited over the farther with its own glass
 *      transmittance. Depth, not fillets.
 *
 * Field is evaluated below device resolution and upscaled: everything here is
 * smooth, and the silhouette stays crisp because alpha is the distance field.
 *
 * In Godot: fragment shader, near verbatim. Node chain as a uniform array,
 * env as a small texture, the shade() body ports line for line.
 *
 * Attributes
 *   size        px (square). Default 200.
 *   theme       "light" | "dark"
 *   state       "idle" | "playing" | "blending" | "thinking"
 *   energy      0..1 amplitude drive
 *   speed       motion multiplier, default 1
 *   hue         hue offset, degrees
 *   turns       twist turns along the path, default 1.6
 *   ribbon      half-width, default 1 (relative)
 *   curl        cross-width curvature in radians, default 0.55
 *   dispersion  chromatic split, default 1
 *   static      present = one frame, no rAF
 *   pose        freeze on the frame the idle animation reaches N seconds in.
 *               Deterministic: the same N is the same drawing, forever. This is
 *               how the fixed logo is pinned. Implies `static`.
 *   field       force the shading field resolution, overriding the 300px
 *               frame-budget ceiling. For offline export only (icon masters).
 */
(function () {
  const TAU = Math.PI * 2;
  const lerp = (a, b, t) => a + (b - a) * t;
  const clamp = (v, a, b) => (v < a ? a : v > b ? b : v);
  const pow5 = (x) => { const x2 = x * x; return x2 * x2 * x; };

  // Dichroic tint along the ribbon: periwinkle body, violet→aqua through the
  // middle. Same identity as the painted mark, applied as glass tint rather
  // than as body colour.
  const RAMP = [
    [0.00, [178, 200, 246]],
    [0.14, [150, 176, 244]],
    [0.28, [126, 142, 240]],
    [0.42, [116,  82, 236]],
    [0.55, [ 66, 200, 214]],
    [0.68, [120, 176, 240]],
    [0.84, [166, 196, 246]],
    [1.00, [192, 214, 250]],
  ];

  function hueMatrix(deg) {
    const a = (deg * Math.PI) / 180, c = Math.cos(a), s = Math.sin(a);
    return [
      0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928,
      0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.140, 0.072 - c * 0.072 - s * 0.283,
      0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072,
    ];
  }

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
      lutR[i] = clamp(r, 0, 255) / 255;
      lutG[i] = clamp(g, 0, 255) / 255;
      lutB[i] = clamp(b, 0, 255) / 255;
    }
  }

  // One light, upper-left, tipped towards the viewer. Blinn half-vector
  // against V = (0,0,1). Every term below comes off this.
  const LL = Math.hypot(-0.44, -0.72, 0.53);
  const lx = -0.44 / LL, ly = -0.72 / LL, lz = 0.53 / LL;
  const HL = Math.hypot(lx, ly, lz + 1);
  const hx = lx / HL, hy = ly / HL, hz = (lz + 1) / HL;

  /**
   * How hard the playing level leans on the twist rate, in rad/s at full scale.
   *
   * The resting rate is 0.62, so at 0.30 a loud passage turns about 1.5x as
   * fast as a quiet one. It was 0.55 (1.9x) while the phase bug was in place,
   * where it was never the thing anyone was actually seeing.
   */
  const ENERGY_TWIST = 0.30;

  /*
   * Track-reactive drives (playing state only).
   *
   * The mark was already a readout of engine state; these make it a readout of
   * the record. Three signals, three distinct things about the ribbon, chosen
   * so none of them is a restatement of another:
   *
   *   beat       -> a kick to the turn rate, decaying inside the beat
   *   brightness -> how fast it turns at rest
   *   level      -> how many turns are in the ribbon at all
   *
   * The last one is geometry, not motion, which is why it can sit alongside the
   * other two without fighting them: `turns` changes the shape being turned,
   * the other two change how fast it is turned. And it is free of the frame
   * fit — `buildPath` bounds on the UN-projected half-width `W`, which does not
   * depend on the twist phase, so winding more turns into the ribbon cannot
   * change its bounding box and cannot make the logo breathe in size.
   */

  /** Turn rate with nothing playing, in rad/s. */
  const TWIST_BASE = 0.55;
  /**
   * Extra turn rate at full bass, in rad/s.
   *
   * Bass rather than brightness, which is what this used to be. A ribbon whose
   * speed came off the treble sped up on hi-hats and ignored the kick, which is
   * the opposite of how the music feels — and left tempo affecting nothing at
   * all, since the beat only ever contributed a decaying kick whose mean is
   * independent of period. 60 and 180 BPM produced the same average speed.
   *
   * There is no separate bass signal to publish: `brightness` is the *high*
   * fraction of block energy, so `1 - brightness` is the low fraction, and
   * multiplying by `level` gives the low-band energy. Both already arrive.
   */
  const TWIST_BASS = 1.10;
  /**
   * How much the tempo raises the resting rate, in rad/s per beat-per-second.
   *
   * The floor, not the reaction. A drum and bass record idles faster than a
   * ballad and still lunges on the kick; without this the two are only
   * distinguishable while something is actually hitting.
   */
  const TWIST_TEMPO = 0.16;
  /** How far brightness shifts the hue, in degrees. */
  const BRIGHT_HUE = 26;
  /**
   * Peak extra turn rate on the beat itself, in rad/s.
   *
   * Decays across the beat, so the mean contribution is much smaller than this
   * — ∫exp(-k·f)df over one beat is (1-e^-k)/k, about 0.2 at the decay below,
   * making the average add ~0.32 rad/s rather than 1.6.
   */
  const TWIST_BEAT = 1.60;
  /** How sharply the beat kick decays across one beat. Higher is snappier. */
  const BEAT_DECAY = 5.0;
  /** Extra turns wound into the ribbon at full level. */
  const TURNS_LEVEL = 0.80;
  /**
   * How far set progress may rotate the hue, in degrees either way.
   *
   * Deliberately narrow. The mark's identity is a fixed periwinkle→violet→aqua
   * ramp and `hueShift` rotates the whole of it, so a wide swing does not read
   * as "further along the set", it reads as a different logo. At ±25° it reads
   * as warmer or cooler, which is the distinction the curve is making.
   */
  const SET_HUE = 25;

  /** Positive remainder. `%` keeps the sign of the dividend, which is wrong here. */
  const fract = (x) => x - Math.floor(x);

  const NODES = 160;
  const NS = NODES - 1;
  const ENV = 40;              // environment texture, ENV×ENV×3

  /**
   * Environment the glass refracts. Not the page — a small idealised studio:
   * a soft vertical gradient, a warm window upper-left, a cool bounce lower
   * right. Glass with nothing to bend looks like plastic, so this exists even
   * on a flat background.
   */
  function buildEnvTex(out, light) {
    const A = light ? [250, 252, 255] : [58, 68, 108];
    const B = light ? [230, 236, 248] : [96, 108, 158];
    const warm = light ? [255, 250, 236] : [122, 132, 196];
    const cool = light ? [216, 234, 250] : [46, 132, 158];
    let i = 0;
    for (let y = 0; y < ENV; y++) {
      const v = y / (ENV - 1);
      for (let x = 0; x < ENV; x++) {
        const u = x / (ENV - 1);
        const g = clamp(v * 0.62 + u * 0.38, 0, 1);
        let r = lerp(A[0], B[0], g), gg = lerp(A[1], B[1], g), b = lerp(A[2], B[2], g);
        const d1 = (u - 0.16) * (u - 0.16) + (v - 0.10) * (v - 0.10);
        const w1 = 1 / (1 + d1 * 14);
        const d2 = (u - 0.92) * (u - 0.92) + (v - 0.96) * (v - 0.96);
        const w2 = 1 / (1 + d2 * 18);
        r = lerp(r, warm[0], w1 * 0.85); gg = lerp(gg, warm[1], w1 * 0.85); b = lerp(b, warm[2], w1 * 0.85);
        r = lerp(r, cool[0], w2 * 0.55); gg = lerp(gg, cool[1], w2 * 0.55); b = lerp(b, cool[2], w2 * 0.55);
        out[i++] = r; out[i++] = gg; out[i++] = b;
      }
    }
  }

  /**
   * The centreline. A loose, lazy waveform — fewer and larger sweeps than the
   * tube mark had, plus a slow secondary wander so no two curves match, plus a
   * z excursion so the ribbon dives behind and rises in front of itself.
   * Everything is closed-form so the tangent can be differentiated smoothly.
   */
  function buildPath(p, out) {
    const at = (t, o) => {
      const th = TAU * (p.f0 * t + p.chirp * t * t) + p.phase;
      const env = Math.exp(-p.damping * t);
      o[0] = (Math.pow(t, 0.95) + 0.036 * Math.sin(TAU * 0.68 * t + p.wander)) * p.xscale;
      o[1] = -env * p.amp * Math.cos(th) + 0.040 * Math.sin(TAU * 0.44 * t - p.wander * 0.7);
      o[2] = p.depth * Math.sin(TAU * 0.62 * t + p.zphase) * (0.42 + 0.58 * t);
    };
    const a = out.tmpA, b = out.tmpB, c = out.tmpC;
    const xs = out.xs, ys = out.ys, zs = out.zs, ph = out.ph, hw = out.hw, ss = out.ss;
    const tx = out.tx, ty = out.ty;
    const h = 0.004;
    for (let i = 0; i < NODES; i++) {
      const t = i / NS;
      at(t, a);
      at(Math.max(0, t - h), b);
      at(Math.min(1, t + h), c);
      xs[i] = a[0]; ys[i] = a[1]; zs[i] = a[2];
      let dx = c[0] - b[0], dy = c[1] - b[1];
      const dl = Math.hypot(dx, dy) || 1;
      tx[i] = dx / dl; ty[i] = dy / dl;
      // Twist: a steady wind plus a slow flutter, so the turns are not metronomic.
      ph[i] = TAU * p.turns * t + p.twist + 0.16 * Math.sin(TAU * 0.75 * t + p.flow);
      // Width: near-constant like a real ribbon, easing off at the two cut ends.
      const endA = clamp(t / 0.07, 0, 1), endB = clamp((1 - t) / 0.09, 0, 1);
      const W = p.width * (0.92 + 0.08 * Math.exp(-1.1 * t)) * (0.66 + 0.34 * endA) * (0.58 + 0.42 * endB);
      out.W[i] = W;
      const cs = Math.cos(ph[i]), sn = Math.sin(ph[i]);
      const th2 = W * 0.24;
      hw[i] = Math.sqrt(W * W * cs * cs + th2 * th2 * sn * sn);
      out.th[i] = th2;
    }
    if (p.tilt) {
      const ang = (p.tilt * Math.PI) / 180, ca = Math.cos(ang), sa = Math.sin(ang);
      for (let i = 0; i < NODES; i++) {
        const qx = xs[i] - 0.5, qy = ys[i];
        xs[i] = qx * ca - qy * sa + 0.5;
        ys[i] = qx * sa + qy * ca;
        const ux = tx[i], uy = ty[i];
        tx[i] = ux * ca - uy * sa; ty[i] = ux * sa + uy * ca;
      }
    }
    ss[0] = 0;
    for (let i = 1; i < NODES; i++) ss[i] = ss[i - 1] + Math.hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
    const tot = ss[NS] || 1;
    for (let i = 0; i < NODES; i++) ss[i] /= tot;
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (let i = 0; i < NODES; i++) {
      const r = out.W[i];
      if (xs[i] - r < minX) minX = xs[i] - r;
      if (xs[i] + r > maxX) maxX = xs[i] + r;
      if (ys[i] - r < minY) minY = ys[i] - r;
      if (ys[i] + r > maxY) maxY = ys[i] + r;
    }
    out.minX = minX; out.maxX = maxX; out.minY = minY; out.maxY = maxY;
  }

  class VaporRibbon extends HTMLElement {
    static get observedAttributes() {
      return ['size', 'theme', 'state', 'energy', 'speed', 'hue', 'turns', 'ribbon', 'curl', 'dispersion', 'static', 'pose', 'xscale', 'field',
              'brightness', 'beat-period', 'beat-at', 'set-energy'];
    }

    constructor() {
      super();
      this._root = this.attachShadow({ mode: 'open' });
      this._canvas = document.createElement('canvas');
      this._canvas.style.display = 'block';
      const st = document.createElement('style');
      st.textContent = ':host{display:inline-block;line-height:0}';
      this._root.append(st, this._canvas);
      this._ctx = this._canvas.getContext('2d');
      this._t0 = performance.now();
      this._raf = 0; this._visible = true; this._energySm = 0.5;
      this._skip = 0; this._tick = 0;
      // Running phases, integrated per frame. See `_phase`.
      this._ph = { twist: 0, wander: 0, zphase: 0, travel: 0 };
      this._lastT = 0;
      this._field = document.createElement('canvas');
      this._fctx = this._field.getContext('2d');
      this._bed = document.createElement('canvas');
      this._bctx = this._bed.getContext('2d');
      this._env = new Float32Array(ENV * ENV * 3);
      this._envLight = null;
      this._P = {
        xs: new Float32Array(NODES), ys: new Float32Array(NODES), zs: new Float32Array(NODES),
        ph: new Float32Array(NODES), hw: new Float32Array(NODES), ss: new Float32Array(NODES),
        tx: new Float32Array(NODES), ty: new Float32Array(NODES), W: new Float32Array(NODES),
        th: new Float32Array(NODES),
        tmpA: new Float64Array(3), tmpB: new Float64Array(3), tmpC: new Float64Array(3),
      };
      // Field-space node chain.
      this._qx = new Float32Array(NODES); this._qy = new Float32Array(NODES);
      this._qz = new Float32Array(NODES); this._qw = new Float32Array(NODES);
      this._qt = new Float32Array(NODES);
      this._o0 = new Float32Array(4); this._o1 = new Float32Array(4);
    }

    connectedCallback() {
      this._resize();
      if ('IntersectionObserver' in window) {
        this._io = new IntersectionObserver((es) => { this._visible = es.some((e) => e.isIntersecting); }, { rootMargin: '80px' });
        this._io.observe(this);
      }
      this._reduced = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      if (this._frozen) this._draw(0); else this._loop();
    }

    disconnectedCallback() {
      cancelAnimationFrame(this._raf);
      if (this._io) this._io.disconnect();
    }

    attributeChangedCallback(name) {
      if (name === 'size' || name === 'field') this._resize();
      if (this._frozen) this._draw(0);
    }

    get _frozen() { return this.hasAttribute('static') || this.hasAttribute('pose') || this._reduced; }

    /**
     * Integrate a rate into a running phase.
     *
     * Multiplying elapsed time by the CURRENT rate is not the same as
     * integrating that rate, and the difference is the whole bug this replaced.
     * For a phase written as T·r(t), the angular velocity is
     *
     *     d/dt [T·r(t)] = r + T·(dr/dt)
     *
     * The second term is spurious and it scales with elapsed time. `playing`
     * drives the twist rate from the smoothed level, so every level poll moved
     * r and whipped the accumulated angle by T·Δr. Measured on the old code:
     * an intended 0.62–1.17 rad/s ran at 58 rad/s a minute in, and went
     * NEGATIVE — the ribbon counter-spun whenever the music got quieter. That
     * is the "turbo twist, slow down, turbo twist" read, and it got worse the
     * longer the screen stayed open.
     *
     * The same error made every state change pop: `playing` and `blending`
     * differ by ~0.3 rad/s of rate, but as phases they differ by T·0.3 — two
     * and a half whole turns, applied in one frame, a minute in. Integrating
     * makes the rate change and leaves the angle where it was, which is what a
     * ribbon that is already turning does.
     *
     * Frozen renders take the closed form instead. They have no history to
     * integrate, and `_energySm` is pinned to its target so every rate is
     * constant, making ∫₀ᵗ r dt = r·t exactly. A given `pose` is the same
     * drawing it always was — the icon masters do not move.
     *
     * Must be called exactly once per channel per frame; twice double-counts.
     */
    /**
     * How far into the current beat we are, as a decaying 1→0 pulse.
     *
     * `beat-at` is a `performance.now()` timestamp for the NEXT beat, not a
     * countdown, so it stays true between the polls that set it — a countdown
     * would be stale the moment it was written. The element runs its own clock
     * off it, which is what keeps this at frame rate without an IPC call per
     * frame.
     *
     * Before that beat lands the difference is negative, and `fract` folds it
     * back into the preceding interval rather than clamping, so the pulse is
     * continuous across the beat instead of restarting at it.
     *
     * Zero when frozen: a pose has no clock, and a posed mark must be the same
     * drawing every time.
     */
    _beatPulse(time) {
      if (this._frozen) return 0;
      const period = this._num('beat-period', 0);
      const at = this._num('beat-at', 0);
      if (!(period > 0) || !(at > 0)) return 0;
      const nowMs = this._t0 + time * 1000;
      return Math.exp(-BEAT_DECAY * fract((nowMs - at) / 1000 / period));
    }

    _phase(name, rate, T, dt) {
      if (this._frozen) return T * rate;
      this._ph[name] += rate * dt;
      return this._ph[name];
    }
    get _size() { return parseFloat(this.getAttribute('size')) || 200; }
    _num(n, d) { const v = parseFloat(this.getAttribute(n)); return isFinite(v) ? v : d; }

    _resize() {
      const s = this._size;
      const dpr = this._dpr = Math.min(window.devicePixelRatio || 1, 2.5);
      this._canvas.width = Math.round(s * dpr);
      this._canvas.height = Math.round(s * dpr);
      this._canvas.style.width = s + 'px';
      this._canvas.style.height = s + 'px';
      // The field is deliberately coarser than the canvas and upscaled — every
      // term here is smooth and alpha IS the distance field, so the silhouette
      // stays crisp. The 300 ceiling is a frame-budget guard for the animated
      // case, not a quality decision, and it is wrong for a one-off export: a
      // 1024px icon master would otherwise be a 300px render blown up. `field`
      // lifts it. Nothing in the app sets it; the icon exporter does.
      const forced = this._num('field', 0);
      const F = Math.round(forced > 0 ? forced : clamp(s * 1.2, 56, 300));
      if (this._F !== F || !this._img) {
        this._field.width = this._field.height = F;
        this._bed.width = this._bed.height = F;
        this._img = this._fctx.createImageData(F, F);
      }
      this._F = F; this._skip = 0;
    }

    _loop() {
      this._raf = requestAnimationFrame(() => this._loop());
      if (!this._visible) return;
      this._draw((performance.now() - this._t0) / 1000);
    }

    _params(time) {
      const state = this.getAttribute('state') || 'idle';
      const speed = this._num('speed', 1);
      const target = clamp(this._num('energy', 0.5), 0, 1);
      if (this._frozen) this._energySm = target;
      else this._energySm += (target - this._energySm) * 0.06;
      const e = this._energySm;
      const T = time * speed;
      // Frame step, in the same scaled seconds as T. Clamped because the loop
      // stops while the element is off-screen or the tab is hidden, and coming
      // back to a ten-second dt would spin the ribbon on the first frame.
      const dt = clamp(T - this._lastT, 0, 0.05);
      this._lastT = T;
      const hue = this._num('hue', 0);

      /*
       * Rates, not phases.
       *
       * Every quantity that used to be written `T * k` is a rate here and goes
       * through `_phase` exactly once, which integrates it. Bounded wobbles
       * (the `Math.sin` terms) stay closed-form: they cannot accumulate error
       * and they are the same drawing either way.
       */
      let twistRate, wanderRate, zphaseRate, travelRate;
      const shape = {};
      if (state === 'playing') {
        /*
         * Driven by the record, not by a timer. See the constants above.
         *
         * `beatPulse` is 1 on the beat and decays to nothing before the next
         * one, so this adds a kick rather than a sustained tempo — the ribbon
         * lunges and coasts, which is what a beat does. It comes off the
         * tracked grid the mixer aligns to, extrapolated locally between polls,
         * so it stays in phase with the record over a whole track rather than
         * drifting the way a nominal BPM would.
         *
         * Level moved OFF the rate and onto `turns` here: loudness and
         * brightness both driving speed made them indistinguishable, and the
         * count of turns is a quantity the ribbon can show that its rate
         * cannot.
         */
        const bright = clamp(this._num('brightness', 0), 0, 1);
        // `e` is the smoothed `energy` attribute, and `energy` is what the
        // level is passed as — there is no separate `level` attribute, and
        // reading one would have been a silent zero.
        const loud = e;
        // Low-band energy: how much of what is playing is bass, times how much
        // is playing at all. The fraction alone would read 1.0 through a quiet
        // sub-only passage and drive the ribbon as hard as a drop.
        const bass = loud * (1 - bright);
        // Beats per second from the tracked grid, so the resting rate belongs
        // to the record rather than to whatever is hitting right now. Zero
        // until a track is analysed, which leaves TWIST_BASE alone.
        const period = this._num('beat-period', 0);
        const bps = period > 0 ? 1 / period : 0;

        twistRate =
          TWIST_BASE +
          TWIST_TEMPO * bps +
          TWIST_BASS * bass +
          TWIST_BEAT * this._beatPulse(time);
        wanderRate = 0.55; zphaseRate = 0.44; travelRate = 0;
        Object.assign(shape, {
          // A little life left in the envelope, well under what it was: the
          // frame fit keys off `amp`, so driving it hard from the level is what
          // used to make the whole mark change size with the music.
          amp: 0.32 + 0.04 * e, depth: 0.34 + 0.16 * e,
          chirp: 0.36 + 0.10 * e,
          turns: this._num('turns', 1.5) + TURNS_LEVEL * e,
          // Where the set is along its curve, not a decorative wobble. Absent
          // (`set-energy` unset, or DJ mode off) this stays on the brand hue.
          // Brightness is a timbre, so it belongs on the timbre channel. A
          // cymbal-heavy passage reads cooler without competing for motion,
          // which is what having it on the rate did.
          hueShift:
            hue +
            SET_HUE * (clamp(this._num('set-energy', 0.5), 0, 1) - 0.5) * 2 +
            BRIGHT_HUE * (bright - 0.5) * 2,
        });
      } else if (state === 'blending') {
        twistRate = 1.15; wanderRate = 0.9; zphaseRate = 0.7; travelRate = 0.45;
        Object.assign(shape, {
          amp: 0.35 + Math.sin(T * 1.6) * 0.04, f0: 0.62 + Math.sin(T * 0.8) * 0.09,
          depth: 0.44, hueShift: hue + T * 24,
        });
      } else if (state === 'thinking') {
        twistRate = 0.9; wanderRate = 0.7; zphaseRate = 0.6; travelRate = 0.95;
        Object.assign(shape, {
          amp: 0.33, chirp: 0.38 + Math.sin(T * 0.7) * 0.12,
          depth: 0.40, hueShift: hue + T * 36,
        });
      } else {                                            // idle — a slow drift
        twistRate = 0.30; wanderRate = 0.30; zphaseRate = 0.22; travelRate = 0;
        Object.assign(shape, {
          amp: 0.34 + Math.sin(T * 0.31) * 0.020,
          hueShift: hue + Math.sin(T * 0.16) * 10,
        });
      }

      // The bounded part of the travel phase, added on top of the integrated
      // part. `playing` and `idle` sway about their current angle rather than
      // ramping away from it.
      const travelOsc =
        state === 'playing' ? Math.sin(T * 0.6) * 0.10
        : state === 'idle' ? Math.sin(T * 0.19) * 0.07
        : 0;
      const twistOsc = state === 'idle' ? Math.sin(T * 0.23) * 0.10 : 0;

      return {
        f0: 0.62, chirp: 0.38, damping: 0.10, tilt: -3, amp: 0.34,
        width: 0.062 * this._num('ribbon', 1),
        xscale: this._num('xscale', 0.82),
        depth: 0.36,
        turns: this._num('turns', 1.5),
        curl: this._num('curl', 0.85), disp: this._num('dispersion', 1),
        hueShift: hue,
        flow: T * 0.7,
        light: (this.getAttribute('theme') || 'light') === 'light',
        ...shape,
        phase: this._phase('travel', travelRate, T, dt) + travelOsc,
        twist: this._phase('twist', twistRate, T, dt) + twistOsc,
        wander: this._phase('wander', wanderRate, T, dt),
        zphase: this._phase('zphase', zphaseRate, T, dt),
      };
    }

    _draw(time) {
      const posed = this.getAttribute('pose');
      if (posed !== null) { const v = parseFloat(posed); time = isFinite(v) ? v : 0; }
      const animated = !this._frozen;
      if (animated && this._skip > 0 && this._tick++ % (this._skip + 1) !== 0) { this._composite(); return; }
      const t0 = performance.now();
      this._shade(this._params(time));
      this._composite();
      if (animated) {
        const ms = performance.now() - t0;
        if (ms > 11 && this._skip < 3) this._skip++;
        else if (ms < 5 && this._skip > 0) this._skip--;
      }
    }

    _shade(p) {
      const F = this._F, img = this._img, data = img.data;
      const P = this._P;
      buildPath(p, P);
      buildLUT(p.hueShift);
      const light = p.light;
      if (this._envLight !== light) { buildEnvTex(this._env, light); this._envLight = light; }
      const env = this._env;

      const pad = F * 0.06;
      const bw = P.maxX - P.minX, bh = P.maxY - P.minY;
      const k = Math.min((F - pad * 2) / bw, (F - pad * 2) / bh);
      const ox = pad - P.minX * k + (F - pad * 2 - bw * k) / 2;
      const oy = pad - P.minY * k + (F - pad * 2 - bh * k) / 2;

      const qx = this._qx, qy = this._qy, qz = this._qz, qw = this._qw, qt = this._qt;
      const ss = P.ss, ph = P.ph, tx = P.tx, ty = P.ty;
      let maxHw = 0, arcPx = 0;
      for (let i = 0; i < NODES; i++) {
        qx[i] = P.xs[i] * k + ox; qy[i] = P.ys[i] * k + oy;
        qz[i] = P.zs[i] * k; qw[i] = P.hw[i] * k; qt[i] = P.th[i] * k;
        if (qw[i] > maxHw) maxHw = qw[i];
        if (i) arcPx += Math.hypot(qx[i] - qx[i - 1], qy[i] - qy[i - 1]);
      }
      const sepArc = clamp(4.0 * maxHw / (arcPx || 1), 0.06, 0.5);   // "a different arm", in arc length

      // Uniform grid over segments, counting-sorted into one contiguous array.
      const cell = Math.max(6, Math.ceil(maxHw * 1.1));
      const gw = Math.ceil(F / cell), gh = gw, nCell = gw * gh;
      if (!this._cnt || this._cnt.length < nCell + 1) this._cnt = new Int32Array(nCell + 1);
      const cnt = this._cnt; cnt.fill(0, 0, nCell + 1);
      if (!this._bx0) {
        this._bx0 = new Int32Array(NS); this._bx1 = new Int32Array(NS);
        this._by0 = new Int32Array(NS); this._by1 = new Int32Array(NS);
        this._rmax = new Float32Array(NS);
        this._sdx = new Float32Array(NS); this._sdy = new Float32Array(NS);
        this._sl = new Float32Array(NS); this._sinv = new Float32Array(NS);
        this._sinv2 = new Float32Array(NS);
        this._jtA = new Float32Array(NS); this._jtB = new Float32Array(NS);
      }
      const bx0 = this._bx0, bx1 = this._bx1, by0 = this._by0, by1 = this._by1, rmax = this._rmax;
      const sdx = this._sdx, sdy = this._sdy, sl = this._sl, sinv = this._sinv, sinv2 = this._sinv2;
      const jtA = this._jtA, jtB = this._jtB;
      let total = 0;
      for (let i = 0; i < NS; i++) {
        const ddx = qx[i + 1] - qx[i], ddy = qy[i + 1] - qy[i];
        const dl2 = ddx * ddx + ddy * ddy, dl = Math.sqrt(dl2);
        sdx[i] = ddx; sdy[i] = ddy; sl[i] = dl;
        sinv[i] = dl > 0 ? 1 / dl : 0; sinv2[i] = dl2 > 0 ? 1 / dl2 : 0;
        rmax[i] = Math.max(qw[i], qw[i + 1]);
        const reach = rmax[i] * 1.65 + 1.5;
        bx0[i] = Math.max(0, Math.floor((Math.min(qx[i], qx[i + 1]) - reach) / cell));
        bx1[i] = Math.min(gw - 1, Math.floor((Math.max(qx[i], qx[i + 1]) + reach) / cell));
        by0[i] = Math.max(0, Math.floor((Math.min(qy[i], qy[i + 1]) - reach) / cell));
        by1[i] = Math.min(gh - 1, Math.floor((Math.max(qy[i], qy[i + 1]) + reach) / cell));
        for (let gy = by0[i]; gy <= by1[i]; gy++) for (let gx = bx0[i]; gx <= bx1[i]; gx++) { cnt[gy * gw + gx + 1]++; total++; }
      }
      // MITER JOINS.
      //
      // Two quads meeting at a turn of Δθ leave an unowned wedge on the outside
      // of the turn. A flat tolerance covers it where the path is lazy and fails
      // where it is sharp — which is exactly where the speckle appeared, in the
      // trough. The wedge is closed exactly by letting each quad reach past its
      // joint by the miter extent hw·tan(Δθ/2), and only penalising overshoot
      // BEYOND that. The two scissored ends get no extent, so they stay flat.
      for (let i = 0; i < NS; i++) {
        let eA = 0, eB = 0;
        if (i > 0) {
          const c1 = (sdx[i - 1] * sdx[i] + sdy[i - 1] * sdy[i]) * sinv[i - 1] * sinv[i];
          eA = Math.tan(Math.acos(clamp(c1, -1, 1)) * 0.5);
        }
        if (i < NS - 1) {
          const c2 = (sdx[i] * sdx[i + 1] + sdy[i] * sdy[i + 1]) * sinv[i] * sinv[i + 1];
          eB = Math.tan(Math.acos(clamp(c2, -1, 1)) * 0.5);
        }
        const cap = rmax[i] * 0.6;
        jtA[i] = Math.min(rmax[i] * eA, cap);
        jtB[i] = Math.min(rmax[i] * eB, cap);
      }

      for (let c = 0; c < nCell; c++) cnt[c + 1] += cnt[c];
      if (!this._items || this._items.length < total) this._items = new Int32Array(total * 2);
      const items = this._items;
      if (!this._cur || this._cur.length < nCell) this._cur = new Int32Array(nCell);
      const cur = this._cur;
      for (let c = 0; c < nCell; c++) cur[c] = cnt[c];
      for (let i = 0; i < NS; i++) {
        for (let gy = by0[i]; gy <= by1[i]; gy++) for (let gx = bx0[i]; gx <= bx1[i]; gx++) items[cur[gy * gw + gx]++] = i;
      }

      // Which cells can hold two DIFFERENT arms? Only those whose segments
      // span enough of the path. Everywhere else — nearly everywhere — the
      // far-arm pass is skipped entirely.
      if (!this._two || this._two.length < nCell) this._two = new Uint8Array(nCell);
      const twoArm = this._two;
      for (let c = 0; c < nCell; c++) {
        const a0 = cnt[c], e0 = cnt[c + 1];
        if (e0 - a0 < 2) { twoArm[c] = 0; continue; }
        let lo = items[a0], hi = items[a0];
        for (let l = a0 + 1; l < e0; l++) {
          const v = items[l];
          if (v < lo) lo = v; else if (v > hi) hi = v;
        }
        twoArm[c] = (ss[hi] - ss[lo]) >= sepArc ? 1 : 0;
      }

      const o0 = this._o0, o1 = this._o1;
      const AA = 1.05;
      const curl = p.curl, disp = p.disp;
      const bodyMul = light ? 1 : 1.28;
      const rimGain = light ? 52 : 205;
      const absorb = light ? 1.55 : 0.72;   // glass density, in ribbon half-widths

      // Shade one arm at one pixel. Everything is local: no height field, no
      // neighbour taps, so nothing about the node discretisation can leak in.
      const shade = (seg, t, d, uSig, sx, sy, out) => {
        const s = ss[seg] + (ss[seg + 1] - ss[seg]) * t;
        const hwv = qw[seg] + (qw[seg + 1] - qw[seg]) * t;
        const thv = qt[seg] + (qt[seg + 1] - qt[seg]) * t;
        let phv = ph[seg], phn = ph[seg + 1];
        phv = phv + (phn - phv) * t;
        let ux = tx[seg] + (tx[seg + 1] - tx[seg]) * t;
        let uy = ty[seg] + (ty[seg + 1] - ty[seg]) * t;
        const ul = Math.hypot(ux, uy) || 1; ux /= ul; uy /= ul;
        // The across-width coordinate arrives signed from the rasteriser:
        // 0 on the centreline, ±1 at the two edges. The SIGN is the whole point
        // — a ribbon bows, so one edge tips towards the viewer and the other
        // away. Unsigned, both edges bend the same way and the strip reads as a
        // pair of parallel tubes.
        const pe = phv + curl * clamp(uSig, -1, 1);
        let nz = Math.cos(pe), sn = Math.sin(pe);
        let nx = -uy * sn, ny = ux * sn;
        if (nz < 0) { nz = -nz; nx = -nx; ny = -ny; }

        const ndl = nx * lx + ny * ly + nz * lz;
        const ndh = nx * hx + ny * hy + nz * hz;
        const diff = ndl > 0 ? ndl : 0;
        // Exponents stay LOW on purpose. The tangent rotates with the
        // waveform and the twist rotates on top of it, so a tight exponent
        // samples that product at high frequency and lays down hard bands
        // across the strip — the stacked-stroke read. Broad lobes integrate it.
        let spec = 0, sheen = 0;
        if (ndh > 0) {
          const a2 = ndh * ndh, a4 = a2 * a2, a8 = a4 * a4, a16 = a8 * a8;
          spec = a16;                   // ^16 glint
          sheen = a2 * ndh;             // ^3 sheen
        }
        const fres = 0.05 + 0.95 * pow5(1 - nz);
        // Optical path through the glass grows as the strip turns edge-on.
        const pathLen = (thv * 2) / (nz + 0.12) + hwv * 0.10;
        const pathN = pathLen / (maxHw || 1);

        // Refracted environment, split per channel. This is what makes it read
        // as glass rather than as a coloured plastic strip.
        const rk = (2.6 + pathLen * 0.55) * disp;
        const ex = sx + nx * rk, ey = sy + ny * rk;
        const sample = (mx, my, o3) => {
          let fx = (mx / F) * (ENV - 1), fy = (my / F) * (ENV - 1);
          fx = fx < 0 ? 0 : fx > ENV - 1 ? ENV - 1 : fx;
          fy = fy < 0 ? 0 : fy > ENV - 1 ? ENV - 1 : fy;
          const x0 = fx | 0, y0 = fy | 0;
          const x1 = x0 < ENV - 1 ? x0 + 1 : x0, y1 = y0 < ENV - 1 ? y0 + 1 : y0;
          const fxx = fx - x0, fyy = fy - y0;
          const i00 = (y0 * ENV + x0) * 3 + o3, i10 = (y0 * ENV + x1) * 3 + o3;
          const i01 = (y1 * ENV + x0) * 3 + o3, i11 = (y1 * ENV + x1) * 3 + o3;
          return lerp(lerp(env[i00], env[i10], fxx), lerp(env[i01], env[i11], fxx), fyy);
        };
        const eR = sample(sx + nx * rk * 0.86, sy + ny * rk * 0.86, 0);
        const eG = sample(ex, ey, 1);
        const eB = sample(sx + nx * rk * 1.16, sy + ny * rk * 1.16, 2);

        const run = clamp(pathN * absorb, 0.16, 3.0);
        const li = clamp(s, 0, 1) * (LUT_N - 1);
        const l0 = li | 0, lf = li - l0, l1 = l0 < LUT_N - 1 ? l0 + 1 : l0;
        const tr = lerp(lutR[l0], lutR[l1], lf);
        const tg = lerp(lutG[l0], lutG[l1], lf);
        const tb = lerp(lutB[l0], lutB[l1], lf);

        // Body: refracted environment seen through `run` units of tinted glass,
        // modulated by how the face meets the light. Colour therefore DEEPENS
        // where the strip turns edge-on and the light has further to travel —
        // which is the tell that reads as glass rather than as tinted plastic.
        const face = 0.40 + 0.68 * diff;
        const aR = 1 / (1 + run * (1 / (tr + 0.004) - 1));
        const aG = 1 / (1 + run * (1 / (tg + 0.004) - 1));
        const aB = 1 / (1 + run * (1 / (tb + 0.004) - 1));
        let r = eR * aR * face;
        let g = eG * aG * face;
        let b = eB * aB * face;
        // Dichroic flash: tint gains strength at grazing angles, as in real
        // coated glass. This is where the violet→aqua identity lives.
        const dic = fres * 0.50 + 0.30;
        r += tr * 214 * dic * 0.28 * bodyMul;
        g += tg * 214 * dic * 0.28 * bodyMul;
        b += tb * 232 * dic * 0.28 * bodyMul;
        // Cool the flank turned away from the light rather than only dimming it.
        const away = ndl < 0 ? -ndl : 0;
        r -= away * 30; g -= away * 20; b -= away * 4;
        // Bright silhouette rim — glass edges catch and pipe light.
        const inside = d < 0 ? -d : 0;
        const rim = 1 / (1 + (inside / 1.15) * (inside / 1.15));
        const rimAmt = rim * (0.22 + 0.78 * fres);
        r += rimAmt * rimGain * 0.94;
        g += rimAmt * rimGain * 0.97;
        b += rimAmt * rimGain;
        const add = spec * 92 + sheen * 22;
        r += add; g += add; b += add;

        out[0] = r; out[1] = g; out[2] = b;
        // Glass is more opaque where it is edge-on, thicker and rimmed.
        const opa = clamp(0.52 + 0.42 * fres + spec * 0.7 + rimAmt * 0.6, 0, 1);
        out[3] = clamp(0.5 - d / AA, 0, 1) * opa;
      };

      let idx = 0;
      for (let y = 0; y < F; y++) {
        const gyb = Math.min(gh - 1, (y / cell) | 0) * gw;
        for (let x = 0; x < F; x++, idx++) {
          const o = idx * 4;
          const c = gyb + Math.min(gw - 1, (x / cell) | 0);
          const cs = cnt[c], ce = cnt[c + 1];
          if (cs >= ce) { data[o + 3] = 0; continue; }
          const sx = x + 0.5, sy = y + 0.5;
          // QUAD STRIP, NOT A DISTANCE FIELD.
          //
          // Each segment owns the quad spanned by its two edge points,
          // centre ± hw·n̂⊥. A pixel belongs to that quad when its projection
          // falls inside the segment (0 ≤ t ≤ 1) AND its perpendicular offset
          // is within hw. That is the ribbon's real silhouette.
          //
          // The previous model took the distance to the centreline and
          // subtracted hw, which unions a DISC of radius hw around every joint.
          // Where the path's radius of curvature approaches hw — the top of
          // each arc — those discs merge into a blob and the strip flares into
          // a paddle with the joints printing through it as a fan. Bounding the
          // offset per quad removes the discs entirely: on the inside of a turn
          // neighbouring quads simply overlap, and on the outside they leave a
          // wedge whose width is hw·Δθ²/8 — well under a pixel at 160 nodes.
          //
          // The representative for an arm is the MINIMUM d, never the maximum
          // depth: many of an arm's own quads cover the same pixel, and picking
          // among them by depth makes the winner hop from quad to quad as the
          // pixel walks the strip — one shading break per node. Depth decides
          // one thing only: which of two separate arms is in front.
          let bd = 1e9, bi = -1, bt = 0, bu = 0;
          for (let l = cs; l < ce; l++) {
            const i = items[l];
            const inv = sinv[i];
            if (inv === 0) continue;
            const bxv = sdx[i], byv = sdy[i], segL = sl[i];
            const wx = sx - qx[i], wy = sy - qy[i];
            let t = (wx * bxv + wy * byv) * sinv2[i];
            // Along-range. Interior quads are strict — their neighbours own
            // anything past the joint. The two terminal quads extend by the AA
            // width so the scissored end gets a soft edge instead of a step.
            let over = 0;
            if (t < 0) { over = -t * segL - jtA[i]; t = 0; if (over < 0) over = 0; }
            else if (t > 1) { over = (t - 1) * segL - jtB[i]; t = 1; if (over < 0) over = 0; }
            const across = (wy * bxv - wx * byv) * inv;
            const hwv = qw[i] + (qw[i + 1] - qw[i]) * t;
            let d = (across < 0 ? -across : across) - hwv;
            if (over > d) d = over;
            if (d >= AA || d >= bd) continue;
            bd = d; bi = i; bt = t; bu = hwv > 0 ? across / hwv : 0;
          }
          if (bi < 0) { data[o + 3] = 0; continue; }

          let fd = 1e9, fi = -1, ft = 0, fu = 0;
          if (twoArm[c] && bd < 0) {
            const sA = ss[bi] + (ss[bi + 1] - ss[bi]) * bt;
            for (let l = cs; l < ce; l++) {
              const i = items[l];
              const inv = sinv[i];
              if (inv === 0) continue;
              const bxv = sdx[i], byv = sdy[i], segL = sl[i];
              const wx = sx - qx[i], wy = sy - qy[i];
              let t = (wx * bxv + wy * byv) * sinv2[i];
              let over = 0;
              if (t < 0) { over = -t * segL - jtA[i]; t = 0; if (over < 0) over = 0; }
              else if (t > 1) { over = (t - 1) * segL - jtB[i]; t = 1; if (over < 0) over = 0; }
              const s2 = ss[i] + (ss[i + 1] - ss[i]) * t;
              const gap = s2 - sA;
              if ((gap < 0 ? -gap : gap) < sepArc) continue;
              const across = (wy * bxv - wx * byv) * inv;
              const hwv = qw[i] + (qw[i + 1] - qw[i]) * t;
              let d = (across < 0 ? -across : across) - hwv;
              if (over > d) d = over;
              if (d >= AA || d >= fd) continue;
              fd = d; fi = i; ft = t; fu = hwv > 0 ? across / hwv : 0;
            }
          }

          // Order the two arms by depth.
          let nI = bi, nT = bt, nD = bd, nU = bu;
          let xI = fi, xT = ft, xD = fd, xU = fu;
          if (fi >= 0) {
            const zA = qz[bi] + (qz[bi + 1] - qz[bi]) * bt;
            const zB = qz[fi] + (qz[fi + 1] - qz[fi]) * ft;
            if (zB > zA) {
              nI = fi; nT = ft; nD = fd; nU = fu;
              xI = bi; xT = bt; xD = bd; xU = bu;
            }
          }

          shade(nI, nT, nD, nU, sx, sy, o0);
          let r = o0[0], g = o0[1], b = o0[2], a = o0[3];
          if (xI >= 0) {
            shade(xI, xT, xD, xU, sx, sy, o1);
            const ab = o1[3] * 0.92;
            const keep = 1 - a;
            // The far arm is seen THROUGH the near glass: cooled and dimmed.
            const bR = o1[0] * 0.80, bG = o1[1] * 0.84, bB = o1[2] * 0.92;
            const outA = a + ab * keep;
            if (outA > 0.0005) {
              r = (r * a + bR * ab * keep) / outA;
              g = (g * a + bG * ab * keep) / outA;
              b = (b * a + bB * ab * keep) / outA;
            }
            a = outA;
          }
          data[o] = r < 0 ? 0 : r > 255 ? 255 : r;
          data[o + 1] = g < 0 ? 0 : g > 255 ? 255 : g;
          data[o + 2] = b < 0 ? 0 : b > 255 ? 255 : b;
          data[o + 3] = a < 0 ? 0 : a > 255 ? 255 : a * 255;
        }
      }
      this._fctx.putImageData(img, 0, 0);
      this._light = light;
    }

    _composite() {
      const ctx = this._ctx, S = this._size, F = this._F, light = this._light !== false;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, this._canvas.width, this._canvas.height);
      ctx.setTransform(this._dpr, 0, 0, this._dpr, 0, 0);
      ctx.imageSmoothingEnabled = true;
      if ('imageSmoothingQuality' in ctx) ctx.imageSmoothingQuality = 'high';

      // Contact shadow + caustic: glass casts a soft dark and a bright focus.
      const b = this._bctx;
      b.setTransform(1, 0, 0, 1, 0, 0);
      b.clearRect(0, 0, F, F);
      b.globalCompositeOperation = 'source-over';
      b.drawImage(this._field, 0, 0);
      b.globalCompositeOperation = 'source-in';
      const bg = b.createLinearGradient(0, 0, F, F);
      if (light) {
        bg.addColorStop(0, 'rgb(120,138,196)');
        bg.addColorStop(0.5, 'rgb(104,150,192)');
        bg.addColorStop(1, 'rgb(136,146,200)');
      } else {
        bg.addColorStop(0, 'rgb(108,142,255)');
        bg.addColorStop(0.5, 'rgb(140,110,250)');
        bg.addColorStop(1, 'rgb(64,214,204)');
      }
      b.fillStyle = bg;
      b.fillRect(0, 0, F, F);
      b.globalCompositeOperation = 'source-over';

      const canBlur = typeof ctx.filter === 'string';
      ctx.save();
      if (canBlur) ctx.filter = `blur(${S * 0.05}px)`;
      ctx.globalAlpha = light ? 0.26 : 0.40;
      ctx.drawImage(this._bed, S * 0.012, S * 0.034, S, S);
      ctx.restore();

      ctx.drawImage(this._field, 0, 0, S, S);
    }
  }

  if (!customElements.get('vapor-ribbon')) customElements.define('vapor-ribbon', VaporRibbon);
})();
