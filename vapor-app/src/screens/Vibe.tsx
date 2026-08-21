/**
 * Vibe DJ.
 *
 * The pathfinder has existed since phase 3 and nothing has ever called it. This
 * is the screen that does: pick a curve, and the library is ordered along it by
 * tempo, key and energy, then handed to the queue.
 *
 * ## What it will not do is pretend
 *
 * The blend panel describes the mix the engine would actually perform, by
 * asking the engine the same questions the audio path asks — so when the tempi
 * are too far apart it says so, rather than showing a confident arrow between
 * two numbers and then playing a hard cut. `matchable` comes from
 * `Mixer::tempo_ratio`, not from a comparison invented here.
 *
 * Energy is integrated loudness, mapped to 0–1. It was a dynamics ratio until
 * 2026-08-17, which measured how *consistent* a track is rather than how hard
 * it goes, and put ballads above drum & bass. See
 * `vapor_library::intensity_from_lufs` — enough to keep a
 * loud ballad and a quiet banger apart, which loudness alone could not.
 */

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { VaporMark } from "../components/VaporMark";
import * as core from "../lib/core";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { Empty } from "../components/States";
import { Queue } from "./Queue";
import { HelpModal } from "../components/HelpModal";
// The spec, verbatim — the same document the Godot help modal rendered from
// res://docs/ai_dj_workflow.md. Imported rather than retyped so it cannot drift.
import workflow from "../../../docs/ai_dj_workflow.md?raw";

/**
 * The four curves, named as `dj_pathfinder.gd` names them.
 *
 * The rewrite had relabelled these: "Chill Down" became "Wind down", and the
 * `_` fallback case — which the original never gave a display name, because it
 * never offered the curves in a UI — became "Steady". Both were inventions
 * (see docs/FINDINGS.md). The ids were always right; only the words drifted.
 *
 * "Hold Steady" is the one label with no origin in the original, since there is
 * nothing to restore it to. Rename it here and nowhere else.
 *
 * Each carried a second line of arithmetic — "Energy +0.4, tempo +15 BPM
 * across the set" — which is true and is not what anyone is choosing between.
 * The curve is a shape, so it is drawn: the line in each swatch is the actual
 * `Curve::target_energy` sampled across the set, and the colour is warm for
 * the ones that climb and cool for the ones that fall.
 */
const CURVES: { id: core.Curve; label: string; line: string }[] = [
  // Sampled from `Curve::target_energy` in pathfinder.rs at t = 0, ¼, ½, ¾, 1,
  // as a path across a 100×32 box. Not decoration: a Wave that drew as a
  // straight line would be a lie about what the planner does.
  { id: "build", label: "Build Vibe", line: "M0,28 L25,22 L50,16 L75,10 L100,4" },
  { id: "chill", label: "Chill Down", line: "M0,4 L25,10 L50,16 L75,22 L100,28" },
  { id: "wave", label: "Wave", line: "M0,16 L25,5 L50,16 L75,27 L100,16" },
  { id: "flat", label: "Hold Steady", line: "M0,16 L25,16 L50,16 L75,16 L100,16" },
];

export function Vibe({
  djMode = true,
  onOpen,
}: {
  /**
   * Whether the DJ is conducting.
   *
   * Switched from the transport, not here — it is a playback control, and the
   * screen that shows what the DJ is doing is a poor place to hide the switch
   * that starts it. Off, this screen is inert and says where the switch is;
   * the queue below stays live, because what is coming next is a fair question
   * in either mode.
   */
  djMode?: boolean;
  onOpen?: ((href: string) => void) | undefined;
} = {}) {
  const [playback, setPlayback] = useState<core.PlaybackState | null>(null);
  /**
   * The beat the mark is pulsing on, on the browser's own clock.
   *
   * Converted here rather than passed through as track-seconds because the
   * mark has no idea where the playhead is: the backend says "the next beat is
   * at 91.4s and we are at 91.1s", and what the renderer can use is "that beat
   * lands 300ms from now". Recomputed on every poll, which is also the drift
   * correction — a tracked grid follows real tempo, so a mark extrapolating
   * from one beat forever would walk out of phase with the record.
   */
  const [beat, setBeat] = useState<{ period: number; at: number } | null>(null);
  const [blend, setBlend] = useState<core.BlendPreview | null>(null);
  const [curve, setCurve] = useState<core.Curve>("build");
  /** True while the planner is re-running, which is a visible pause. */
  const [planning, setPlanning] = useState(false);
  /** The three ways out of the playing track (docs/ai_dj_workflow.md §2–§4). */
  const [candidates, setCandidates] = useState<core.MixCandidate[]>([]);
  const [helping, setHelping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    const [p, b, c] = await Promise.allSettled([
      core.playbackState(),
      core.blendPreview(),
      core.mixCandidates(),
    ]);
    if (p.status === "fulfilled") {
      setPlayback(p.value);
      const v = p.value;
      setBeat(
        v.beatPeriod > 0 && v.nextBeat > v.position
          ? {
              period: v.beatPeriod,
              at: performance.now() + (v.nextBeat - v.position) * 1000,
            }
          : null,
      );
    }
    if (b.status === "fulfilled") setBlend(b.value);
    if (c.status === "fulfilled") setCandidates(c.value);
  }, []);

  // Read once, for the curve.
  useEffect(() => {
    core
      .settings()
      .then((s) => {
        // The curve is the backend's, not this component's: the playback
        // thread extends the set along it whether or not this screen is open,
        // so showing a local default would show the wrong one.
        setCurve(core.asCurve(s.curve));
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 1000);
    const unlisten = listen("playback-changed", () => void refresh());
    return () => {
      clearInterval(timer);
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  /**
   * Choose where the set is going.
   *
   * There is no "Conduct" button any more. There was one, and it was the only
   * thing that ever ran the planner — so a set was something you had to know to
   * ask for, and the DJ otherwise appended one track at a time with no idea
   * where it was heading. Now the backend plans on its own and this only says
   * where to; pressing a curve re-plans everything after the track playing.
   */
  async function choose(next: core.Curve) {
    setCurve(next);
    setPlanning(true);
    setError(null);
    try {
      // Returns as soon as the choice is saved; the route is planned in the
      // background and announced with `playback-changed`, which `refresh` is
      // already listening for. Awaiting the plan here is what made the press
      // take five seconds to land.
      await core.setCurve(next);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
      // The press did not take, so the row should not go on claiming it did.
      setCurve(curve);
    } finally {
      setPlanning(false);
    }
  }

  /**
   * Take one of the three exits by hand.
   *
   * Follow is already what is queued, so pressing it is a no-op by
   * construction; Stay and Switch move the set onto a different next track and
   * re-plan the tail from there along the same curve.
   */
  async function pick(candidate: core.MixCandidate) {
    setError(null);
    try {
      await core.chooseNext(candidate.href, curve);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }

  /*
   * The document supplies its own title, so it is lifted out of the body
   * rather than retyped as a string here — otherwise renaming the spec leaves
   * a stale heading on the sheet.
   */
  const firstHeading = /^#\s+(.+)$/m.exec(workflow);
  const helpTitle = firstHeading?.[1] ?? "Smart Mixing";
  const helpBody = firstHeading
    ? workflow.replace(firstHeading[0], "")
    : workflow;

  /* Reachable from both states of this screen. Nothing playing is precisely
   * when someone wants to read what the DJ is going to do.
   *
   * The button and the sheet are separate because the button goes *inside* the
   * `<h1>` and the sheet must not: a heading may only hold phrasing content,
   * and — the part that showed — CSS `letter-spacing` in `em` resolves where it
   * is declared and then inherits as pixels, so the title's `-0.03em` reached
   * the sheet as `-1.38px` and set fourteen-pixel body text at display
   * tracking. The document was unreadable and the cause was three elements
   * away from it. */
  const helpButton = (
    <button
      className="vibe__help"
      onClick={() => setHelping(true)}
      aria-label="How the DJ chooses"
      title="How the DJ chooses"
    >
      ?
    </button>
  );
  const helpSheet = helping && (
    <HelpModal
      title={helpTitle}
      markdown={helpBody}
      onClose={() => setHelping(false)}
    />
  );

  if (playback && !playback.href && !playback.loading) {
    return (
      <div className="vibe vibe--empty">
        <div className="vibe__help-row">{helpButton}</div>
        {helpSheet}
        <Empty
          title="Nothing to conduct yet"
          body="Start a track, then the DJ can plan a set around it."
        />
      </div>
    );
  }

  const mixing = playback?.mixing ?? false;
  /*
   * What the DJ is conducting over.
   *
   * It said "this device" — which is where the audio comes out, not what is
   * being conducted, and was the same words whatever you pressed play in. The
   * scope comes from the list the queue was started from and is the pool the
   * planner is actually confined to (see `AppState::scope`), so this names the
   * thing rather than describing the hardware.
   */
  const scope = playback?.scope?.trim();

  return (
    <div className="vibe">
      <header className="vibe__head">
        <div className="vibe__mark">
          <VaporMark
            size={92}
            // The mark *is* the DJ on this screen: it tightens while planning
            // and swirls through a blend, both of which are real states.
            state={planning ? "thinking" : mixing ? "blending" : "playing"}
            energy={playback?.level ?? 0}
            // And the record, not just the engine. Loudness winds turns into
            // the ribbon, brightness sets how fast it turns, the beat kicks it,
            // and the hue says how far along the curve the set has got.
            brightness={playback?.brightness}
            beatPeriod={beat?.period}
            beatAt={beat?.at}
            // Absent in shuffle, where there is no curve to be partway along.
            setEnergy={
              playback && playback.setTotal > 0 ? playback.setEnergy : undefined
            }
          />
        </div>
        <div className="vibe__heading">
          <h1 className="vibe__title">
            {djMode ? "Vibe" : "Shuffle"}
            {helpButton}
          </h1>
          <p className="label">
            {!djMode
              ? "playing in queue order"
              : scope
                ? `conducting on ${scope}`
                : "conducting on your library"}
          </p>
        </div>
      </header>

      {/*
        The DJ's own controls, shown either way.

        Off, they are dimmed and covered rather than removed. Removing them
        left a screen with a queue on it and no sign that the thing it is named
        after exists, so the one question it had to answer — where do I turn
        this on — was answered by an empty space. Inert, not just faint:
        `inert` takes the whole subtree out of tab order and off the pointer,
        so nothing behind the overlay can be reached by accident.

        The queue below is never covered: what is coming next is the question
        this screen answers in both modes.
      */}
      <div className={"vibe__dj-panel" + (djMode ? "" : " vibe__dj-panel--off")}>
        <div className="vibe__dj-controls" inert={!djMode}>
          {/* Three cards, not three rows in a panel. The sleeve is the biggest
              thing anyone recognises about a track, and it was a 40px square at
              the left of a list; the choice is between three records, so the
              records are what the screen shows. No container around them —
              a white box behind album art is a box, not a design. */}
          {candidates.length === 0 ? (
            <p className="vibe__note">Nothing analysed to choose from yet.</p>
          ) : (
            <ul className="vibe__exits">
              {candidates.map((c) => (
                <li key={c.href}>
                  <button
                    className={
                      "vibe__exit vibe__exit--" +
                      c.exit +
                      (c.selected ? " vibe__exit--on" : "")
                    }
                    aria-pressed={c.selected}
                    onClick={() => void pick(c)}
                  >
                    <span className="vibe__exit-top">
                      <span className="vibe__exit-art" aria-hidden="true">
                        {c.cover && <img src={c.cover} alt="" />}
                      </span>
                      <span className="vibe__exit-word">{c.label}</span>
                    </span>
                    <span className="vibe__exit-title">{c.title}</span>
                    <span className="vibe__exit-artist">{c.artist || "—"}</span>
                    {/* One mono line: tempo, key, and the mix that gets you
                        there. */}
                    <span className="vibe__exit-facts numeric">
                      <span>{fmtBpm(c.bpm)}</span>
                      <span className="vibe__dot">·</span>
                      <span>{c.key || "—"}</span>
                      <span className="vibe__dot">·</span>
                      <span>{c.transition}</span>
                    </span>
                    {/* What the blend actually costs, on its own line rather than
                        appended to the one above — appended, it was the first
                        thing the ellipsis ate, and "no beat match" is the one
                        fact on the card worth reading. Only on the queued card,
                        because it is the only blend the engine has planned. */}
                    {c.selected && blend && (
                      <span
                        className={
                          "vibe__exit-blend numeric" +
                          (blend.matchable ? "" : " vibe__exit-warn")
                        }
                      >
                        {blend.matchable
                          ? `${blend.shiftPercent >= 0 ? "+" : ""}${blend.shiftPercent.toFixed(1)}% to beat match`
                          : "no beat match"}
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}

          <section className="vibe__card glass">
            <h2 className="label">where the set is going</h2>
            <div className="vibe__curves">
              {CURVES.map((c) => (
                <button
                  key={c.id}
                  className={
                    "vibe__curve vibe__curve--" +
                    c.id +
                    (curve === c.id ? " vibe__curve--on" : "")
                  }
                  aria-pressed={curve === c.id}
                  onClick={() => void choose(c.id)}
                  title={c.label}
                >
                  <svg
                    className="vibe__curve-shape"
                    viewBox="0 0 100 32"
                    preserveAspectRatio="none"
                    aria-hidden="true"
                  >
                    <path d={c.line} />
                  </svg>
                  {/* Read, not seen. The drawing is the label — which is what
                      the swatch was for — and four names at a phone's width
                      wrapped the row onto two. */}
                  <span className="a11y-only">{c.label}</span>
                </button>
              ))}
            </div>
            {error && (
              <ErrorNotice error={error} onDismiss={() => setError(null)} />
            )}
          </section>
        </div>

        {!djMode && (
          <div className="vibe__dj-off">
            {/* The same mark as the switch that fixes it, so the sentence and
                the control are recognisably about one thing. */}
            <span className="icon icon--vibe vibe__dj-off-mark" aria-hidden="true" />
            <p className="vibe__dj-off-text">
              Please enable Vibe DJ in your player
            </p>
          </div>
        )}
      </div>

      {helpSheet}

      {/* The queue, in the screen the design puts it in — its mockup reads
          "Up next · Conducted by Vibe · 47 min", with a Re-conduct action on
          it. It was a sidebar destination of its own instead. */}
      <Queue onOpen={onOpen} conducted={djMode} />
    </div>
  );
}

function fmtBpm(bpm: number): string {
  return bpm > 0 ? String(Math.round(bpm)) : "—";
}
