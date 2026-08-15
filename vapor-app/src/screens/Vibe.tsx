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

const CURVES: { id: core.Curve; label: string; blurb: string }[] = [
  { id: "build", label: "Build", blurb: "Starts easy, ends hard" },
  { id: "chill", label: "Wind down", blurb: "Lets the energy fall away" },
  { id: "wave", label: "Wave", blurb: "Rises and falls across the set" },
  { id: "flat", label: "Steady", blurb: "Holds one mood throughout" },
];

export function Vibe() {
  const [playback, setPlayback] = useState<core.PlaybackState | null>(null);
  const [blend, setBlend] = useState<core.BlendPreview | null>(null);
  const [curve, setCurve] = useState<core.Curve>("build");
  const [conducting, setConducting] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [p, b] = await Promise.allSettled([
      core.playbackState(),
      core.blendPreview(),
    ]);
    if (p.status === "fulfilled") setPlayback(p.value);
    if (b.status === "fulfilled") setBlend(b.value);
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
            ? ` ${path.skipped.toLocaleString()} were passed over — not analysed yet.`
            : ""),
      );
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    } finally {
      setConducting(false);
    }
  }

  if (playback && !playback.href && !playback.loading) {
    return (
      <Empty
        title="Nothing to conduct yet"
        body="Start a track, then the DJ can plan a set around it."
      />
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
          <h1 className="vibe__title">Vibe</h1>
          <p className="label">conducting on this device</p>
        </div>
      </header>

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
