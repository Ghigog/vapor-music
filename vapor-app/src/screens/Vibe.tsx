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
 * Energy is loudness, brightness and tempo in equal parts — enough to keep a
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
 * (see docs/DESIGN_DRIFT.md). The ids were always right; only the words drifted.
 *
 * "Hold Steady" is the one label with no origin in the original, since there is
 * nothing to restore it to. Rename it here and nowhere else.
 *
 * The second line is the curve's actual arithmetic, taken from
 * `Curve::target_energy` / `Curve::target_bpm` in `pathfinder.rs`, rather than
 * the prose the rewrite had here ("Starts easy, ends hard"). What the set does
 * is a measurement, and this screen is where a person decides based on it.
 */
const CURVES: { id: core.Curve; label: string; blurb: string }[] = [
  { id: "build", label: "Build Vibe", blurb: "Energy +0.4, tempo +15 BPM across the set" },
  { id: "chill", label: "Chill Down", blurb: "Energy −0.4, tempo −15 BPM across the set" },
  { id: "wave", label: "Wave", blurb: "Energy ±0.3, tempo ±10 BPM, one full cycle" },
  { id: "flat", label: "Hold Steady", blurb: "Holds the starting energy and tempo" },
];

export function Vibe({
  djMode = true,
  onDjModeChange,
  onOpen,
}: {
  /**
   * Whether the DJ is conducting.
   *
   * Off, the queue plays in its own order and this screen is the shuffle
   * screen — which is what the Daylight design calls it, relabelling the tab
   * from "Vibe" to "Shuffle" (docs/DESIGN_DRIFT.md). The queue is shown either
   * way, because the question "what is coming next, and who decided" has the
   * same answer in both modes and it should not need two screens.
   */
  djMode?: boolean;
  onDjModeChange?: ((on: boolean) => void) | undefined;
  onOpen?: ((href: string) => void) | undefined;
} = {}) {
  const [playback, setPlayback] = useState<core.PlaybackState | null>(null);
  const [blend, setBlend] = useState<core.BlendPreview | null>(null);
  const [curve, setCurve] = useState<core.Curve>("build");
  const [conducting, setConducting] = useState(false);
  /** The three ways out of the playing track (docs/ai_dj_workflow.md §2–§4). */
  const [candidates, setCandidates] = useState<core.MixCandidate[]>([]);
  const [helping, setHelping] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [p, b, c] = await Promise.allSettled([
      core.playbackState(),
      core.blendPreview(),
      core.mixCandidates(),
    ]);
    if (p.status === "fulfilled") setPlayback(p.value);
    if (b.status === "fulfilled") setBlend(b.value);
    if (c.status === "fulfilled") setCandidates(c.value);
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

  async function conduct() {
    if (!playback?.href) return;
    setConducting(true);
    setError(null);
    setResult(null);
    try {
      const path = await core.vibePath(playback.href, curve);
      if (path.hrefs.length <= 1) {
        setError(
          "Not enough analysed tracks to plan a set. Analyse more of the library in Settings.",
        );
        return;
      }
      // The path starts at what is already playing, so setting it as the queue
      // reorders what comes next without interrupting the current track.
      await core.playTracks(path.hrefs, playback.href);
      setResult(
        `${path.hrefs.length} tracks, chosen from ${path.considered.toLocaleString()} the DJ has listened to.` +
          // Silence about the rest would make a set built from a tenth of the
          // library look identical to one built from all of it.
          (path.skipped > 0
            ? ` ${path.skipped.toLocaleString()} ${
                path.skipped === 1 ? "was" : "were"
              } passed over — not analysed yet.`
            : ""),
      );
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    } finally {
      setConducting(false);
    }
  }

  /**
   * Take one of the three exits by hand.
   *
   * The badge stays on whatever the DJ would have chosen, so the override is
   * visible as an override rather than rewriting history — the original's
   * "AI Choice" badge behaviour.
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
   * when someone wants to read what the DJ is going to do. */
  const help = (
    <>
      <button className="vibe__help" onClick={() => setHelping(true)}>
        Help
      </button>
      {helping && (
        <HelpModal
          title={helpTitle}
          markdown={helpBody}
          onClose={() => setHelping(false)}
        />
      )}
    </>
  );

  if (playback && !playback.href && !playback.loading) {
    return (
      <div className="vibe vibe--empty">
        <div className="vibe__help-row">{help}</div>
        <Empty
          title="Nothing to conduct yet"
          body="Start a track, then the DJ can plan a set around it."
        />
      </div>
    );
  }

  const mixing = playback?.mixing ?? false;

  return (
    <div className="vibe">
      <header className="vibe__head">
        <div className="vibe__mark">
          <VaporMark
            size={92}
            theme="light"
            // The mark *is* the DJ on this screen: it tightens while planning
            // and swirls through a blend, both of which are real states.
            state={conducting ? "thinking" : mixing ? "blending" : "playing"}
            energy={playback?.level ?? 0}
          />
        </div>
        <div>
          <h1 className="vibe__title">{djMode ? "Vibe" : "Shuffle"}</h1>
          <p className="label">
            {djMode ? "conducting on this device" : "playing in queue order"}
          </p>
        </div>
        <div className="vibe__head-actions">
          {onDjModeChange && (
            <label className="vibe__dj">
              <input
                type="checkbox"
                checked={djMode}
                onChange={(e) => onDjModeChange(e.target.checked)}
              />
              <span>DJ</span>
            </label>
          )}
          {help}
        </div>
      </header>

      {/* Only the DJ's own controls are conditional. The queue below is
          not: what is coming next is the question this screen answers in
          either mode, and it is the only thing that says which mode you
          are actually in. */}
      {djMode && (
        <>
        <section className="vibe__card glass">
          <h2 className="label">next blend</h2>
          {blend ? (
            <>
              <div className="vibe__blend">
                <div className="vibe__side">
                  <span className="vibe__side-label label">out</span>
                  <span className="vibe__side-title">{blend.fromTitle || "—"}</span>
                  <span className="vibe__side-facts numeric">
                    {fmtBpm(blend.fromBpm)} · {blend.fromKey || "—"}
                  </span>
                </div>
                <div
                  className={
                    "vibe__arrow" + (blend.matchable ? "" : " vibe__arrow--no")
                  }
                >
                  <span className="vibe__verdict label">
                    {blend.matchable ? blend.transition : blend.reason}
                  </span>
                  <span aria-hidden="true">→</span>
                </div>
                <div className="vibe__side vibe__side--in">
                  <span className="vibe__side-label label">in</span>
                  <span className="vibe__side-title">{blend.toTitle || "—"}</span>
                  <span className="vibe__side-facts numeric">
                    {fmtBpm(blend.toBpm)} · {blend.toKey || "—"}
                  </span>
                </div>
              </div>

              <dl className="vibe__stats">
                <Stat
                  k="shift"
                  v={
                    blend.matchable
                      ? `${blend.shiftPercent >= 0 ? "+" : ""}${blend.shiftPercent.toFixed(1)}%`
                      : "—"
                  }
                />
                <Stat
                  k="gain"
                  v={`${blend.gainDelta >= 0 ? "+" : ""}${blend.gainDelta.toFixed(1)} LU`}
                />
                <Stat k="key" v={`${blend.fromKey || "—"} → ${blend.toKey || "—"}`} />
              </dl>

              {!blend.matchable && (
                <p className="vibe__note">
                  These two play one after the other instead. The engine refuses a
                  stretch past ±6% — past that it stops sounding like the record.
                </p>
              )}
            </>
          ) : (
            <p className="vibe__note">
              Nothing queued after this track, so there is no blend to describe.
            </p>
          )}
        </section>

        <section className="vibe__card glass">
          <h2 className="label">or blend into</h2>
          {candidates.length === 0 ? (
            <p className="vibe__note">
              Nothing analysed to choose from yet. The DJ needs a tempo and a
              key before it can offer a way out of this track.
            </p>
          ) : (
            <ul className="vibe__picks">
              {candidates.map((c) => (
                <li key={c.href}>
                  <button
                    className={
                      "vibe__pick" + (c.selected ? " vibe__pick--on" : "")
                    }
                    aria-pressed={c.selected}
                    onClick={() => void pick(c)}
                  >
                    <span className="vibe__pick-art" aria-hidden="true">
                      {c.cover && <img src={c.cover} alt="" />}
                    </span>
                    <span className="vibe__pick-text">
                      <span className="vibe__pick-title">
                        {c.title}
                        {c.aiChoice && (
                          <span className="vibe__ai">AI choice</span>
                        )}
                      </span>
                      {/* The design's one mono line: tempo, key, and the mix
                          that gets you there. */}
                      <span className="vibe__pick-facts numeric">
                        <span>{fmtBpm(c.bpm)}</span>
                        <span className="vibe__dot">·</span>
                        <span>{c.key || "—"}</span>
                        <span className="vibe__dot">·</span>
                        <span>{c.transition}</span>
                      </span>
                    </span>
                    <span className={"vibe__tag vibe__tag--" + c.kind}>
                      {c.label}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <p className="vibe__note">
            The curve decides where the set is going; this decides the next step.
            Picking one re-plans the rest along the same curve.
          </p>
        </section>

        <section className="vibe__card glass">
          <h2 className="label">conduct a set</h2>
          <div className="vibe__curves">
            {CURVES.map((c) => (
              <button
                key={c.id}
                className={
                  "vibe__curve" + (curve === c.id ? " vibe__curve--on" : "")
                }
                onClick={() => setCurve(c.id)}
              >
                <span className="vibe__curve-label">{c.label}</span>
                <span className="vibe__curve-blurb">{c.blurb}</span>
              </button>
            ))}
          </div>
          <button
            className="vibe__go"
            onClick={() => void conduct()}
            disabled={conducting || !playback?.href}
          >
            {conducting ? "Choosing…" : "Conduct from here"}
          </button>
          {result && <p className="vibe__result">{result}</p>}
          {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}
          <p className="vibe__note">
            Energy is loudness, brightness and tempo in equal parts — enough to
            tell a loud ballad from a quiet banger, which loudness alone cannot.
          </p>
        </section>
        </>
      )}

      {/* The queue, in the screen the design puts it in — its mockup reads
          "Up next · Conducted by Vibe · 47 min", with a Re-conduct action on
          it. It was a sidebar destination of its own instead. */}
      <Queue onOpen={onOpen} conducted={djMode} />
    </div>
  );
}

function Stat({ k, v }: { k: string; v: string }) {
  return (
    <div className="vibe__stat">
      <dt className="label">{k}</dt>
      <dd className="numeric">{v}</dd>
    </div>
  );
}

function fmtBpm(bpm: number): string {
  return bpm > 0 ? String(Math.round(bpm)) : "—";
}
