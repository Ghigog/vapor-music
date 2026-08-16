/**
 * Syncing with another device on the same Wi-Fi — SYNC-005.
 *
 * Lives in Settings beside "Your data", because that is the claim it belongs
 * to: the library is on the owner's own storage and this moves it between
 * their own machines directly, without anything in the middle. It is the one
 * screen where that promise is visible as a mechanism rather than a sentence.
 *
 * ## Pairing runs on the device that is *offering*
 *
 * The device you want to pair *with* shows a code; you type it on the other
 * one. That is why there are two controls per row — "Show a code" for when
 * this machine is the one being read off, and "Enter a code" for when it is
 * the one being typed into. A single button would have to guess which end of
 * the exchange this device is, and it cannot.
 */
import { useCallback, useEffect, useState } from "react";
import * as core from "../lib/core";
import { ErrorNotice, messageOf } from "./ErrorNotice";

export function SyncPanel() {
  const [view, setView] = useState<core.SyncView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  /** The peer whose code is being typed here, if any. */
  const [entering, setEntering] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [what, setWhat] = useState<core.SyncWhat>({
    tracks: true,
    playlists: true,
  });
  const [shared, setShared] = useState<"idle" | "running">("idle");
  const [sharedNote, setSharedNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setView(await core.syncView());
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }, []);

  // Polled rather than pushed: discovery is a beacon every five seconds, so
  // there is no moment for the backend to announce — a device simply stops
  // being heard from.
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, [refresh]);

  async function show(peer: core.SyncPeer) {
    setError(null);
    try {
      await core.openPairing(peer.id);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }

  async function enter(peerId: string) {
    setError(null);
    try {
      const name = await core.pairWith(peerId, code.trim());
      setEntering(null);
      setCode("");
      setNote(`Paired with ${name}.`);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }

  async function sync(peer: core.TrustedDevice) {
    setError(null);
    setNote(null);
    try {
      await core.syncWith(peer.id, what);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }

  /** SYNC-006: the round trip through the WebDAV server. */
  async function syncShared() {
    setShared("running");
    setSharedNote(null);
    setError(null);
    try {
      const r = await core.syncSharedDocument();
      const changes = [
        r.playlistsAdded &&
          `${r.playlistsAdded} playlist${r.playlistsAdded === 1 ? "" : "s"} arrived`,
        r.playlistsExtended && `${r.playlistsExtended} gained tracks`,
        r.foldersAdded &&
          `${r.foldersAdded} folder${r.foldersAdded === 1 ? "" : "s"} arrived`,
        r.temposAdded &&
          `${r.temposAdded} tempo correction${r.temposAdded === 1 ? "" : "s"} arrived`,
      ].filter(Boolean);

      // Four outcomes, and three of them look identical if only the changes
      // are reported: nothing there yet, nothing to change, and a failure that
      // did not throw.
      setSharedNote(
        r.created
          ? "Written for the first time. Other devices will pick it up when they sync."
          : changes.length > 0
            ? `${changes.join(", ")}. Yours is on the server too.`
            : "Already in step. Yours is on the server too.",
      );
      window.dispatchEvent(new Event("vapor:playlists-changed"));
    } catch (e: unknown) {
      setError(messageOf(e));
    } finally {
      setShared("idle");
    }
  }

  async function forget(peer: core.TrustedDevice) {
    if (
      !window.confirm(
        `Stop syncing with ${peer.name}? It will have to be paired again, with a new code.`,
      )
    ) {
      return;
    }
    try {
      await core.forgetPeer(peer.id);
      await refresh();
    } catch (e: unknown) {
      setError(messageOf(e));
    }
  }

  if (!view) return null;

  const trustedIds = new Set(view.trusted.map((t) => t.id));
  const unpaired = view.discovered.filter((p) => !trustedIds.has(p.id));
  const progress = view.progress;

  return (
    <section className="settings__card glass">
      <h2 className="settings__section">Your other devices</h2>

      <p className="settings__hint">
        Vapor on another machine on this Wi-Fi can copy tracks and playlists
        straight across. Nothing goes through a server, and a device has to be
        paired with a code before it can see anything at all.
      </p>
      <p className="settings__stat numeric">
        This device is <strong>{view.deviceName}</strong>
      </p>

      {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}

      {/*
        Off by default, for the same reason lookups are: a beacon every five
        seconds announces this machine to whatever network it is joined to.
        Nothing listens until this is on, so there is no firewall prompt and
        nothing accepting connections either.
      */}
      <label className="settings__switch">
        <input
          type="checkbox"
          checked={view.enabled}
          onChange={(e) => {
            const on = e.target.checked;
            void core
              .setSyncEnabled(on)
              .then(refresh)
              .then(() =>
                setNote(
                  on
                    ? "This device is now visible to other copies of Vapor on this network."
                    : "Switched off. Nothing is announced, nothing is listening, and paired devices have been forgotten.",
                ),
              )
              .catch((err: unknown) => setError(messageOf(err)));
          }}
        />
        <span>Find my other devices on this network</span>
      </label>

      {!view.enabled && note && <p className="settings__note">{note}</p>}
      {!view.enabled && (
        <p className="settings__hint">
          Turning this on lets Vapor on your other machines find this one. They
          still have to be paired with a code before either can see the other's
          library.
        </p>
      )}

      {view.enabled && (
        <>
          {/* The code this device is showing, for someone to type on the other. */}
          {view.pin && (
            <div className="sync__pin">
              <span className="label">show this on the other device</span>
              <strong className="sync__pin-code numeric">{view.pin}</strong>
              <button
                className="settings__button"
                onClick={() => void core.cancelPairing().then(refresh)}
              >
                Done
              </button>
            </div>
          )}

          <h3 className="label sync__heading">paired</h3>
          {view.trusted.length === 0 ? (
            <p className="settings__hint">
              None yet. Pair one below, and it stays paired until you say
              otherwise.
            </p>
          ) : (
            <ul className="sync__list">
              {view.trusted.map((peer) => {
                const here = view.discovered.some((p) => p.id === peer.id);
                return (
                  <li key={peer.id} className="sync__row">
                    <span
                      className={"sync__dot" + (here ? " sync__dot--on" : "")}
                      aria-hidden="true"
                    />
                    <span className="sync__name">
                      {peer.name}
                      {/* A paired device that is switched off is not a failure,
                      and a row that says nothing about it reads as one. */}
                      <span className="sync__where">
                        {here ? "on this network" : "not on this network"}
                      </span>
                    </span>
                    <button
                      className="settings__button"
                      disabled={!here || progress.running}
                      onClick={() => void sync(peer)}
                    >
                      {progress.running && progress.peer === peer.name
                        ? "Syncing…"
                        : "Sync now"}
                    </button>
                    <button
                      className="settings__button settings__button--danger"
                      onClick={() => void forget(peer)}
                    >
                      Forget
                    </button>
                  </li>
                );
              })}
            </ul>
          )}

          <h3 className="label sync__heading">on this network</h3>
          {unpaired.length === 0 ? (
            <p className="settings__hint">
              Nothing else running Vapor here. Both devices need to be on the
              same Wi-Fi, and some networks block devices from seeing each other
              at all.
            </p>
          ) : (
            <ul className="sync__list">
              {unpaired.map((peer) => (
                <li key={peer.id} className="sync__row">
                  <span
                    className="sync__dot sync__dot--on"
                    aria-hidden="true"
                  />
                  <span className="sync__name">
                    {peer.name}
                    <span className="sync__where numeric">{peer.address}</span>
                  </span>
                  {entering === peer.id ? (
                    <>
                      <input
                        className="settings__input sync__code"
                        autoFocus
                        inputMode="numeric"
                        aria-label={`Code shown on ${peer.name}`}
                        placeholder="000000"
                        value={code}
                        onChange={(e) => setCode(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") void enter(peer.id);
                          if (e.key === "Escape") setEntering(null);
                        }}
                      />
                      <button
                        className="settings__button"
                        onClick={() => void enter(peer.id)}
                      >
                        Pair
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        className="settings__button"
                        onClick={() => void show(peer)}
                      >
                        Show a code
                      </button>
                      <button
                        className="settings__button"
                        onClick={() => {
                          setCode("");
                          setEntering(peer.id);
                        }}
                      >
                        Enter a code
                      </button>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )}

          <h3 className="label sync__heading">what moves</h3>
          <label className="settings__switch">
            <input
              type="checkbox"
              checked={what.tracks}
              onChange={(e) => setWhat({ ...what, tracks: e.target.checked })}
            />
            <span>Track files</span>
          </label>
          <label className="settings__switch">
            <input
              type="checkbox"
              checked={what.playlists}
              onChange={(e) =>
                setWhat({ ...what, playlists: e.target.checked })
              }
            />
            <span>Playlists</span>
          </label>

          {(progress.running || progress.total > 0 || progress.error) && (
            <div className="sync__progress">
              {progress.error ? (
                <ErrorNotice error={progress.error} />
              ) : (
                <>
                  <div
                    className="sync__bar"
                    role="progressbar"
                    aria-valuenow={progress.done}
                    aria-valuemin={0}
                    aria-valuemax={progress.total}
                    aria-label="Sync progress"
                  >
                    <span
                      className="sync__bar-fill"
                      style={{
                        width: `${progress.total > 0 ? (progress.done / progress.total) * 100 : 0}%`,
                      }}
                    />
                  </div>
                  <p className="settings__hint numeric">
                    {progress.done} of {progress.total}
                    {progress.file && ` · ${progress.file}`}
                    {/* Divided here rather than pushed from the backend, which
                    would arrive already stale. */}
                    {progress.elapsed > 0 &&
                      ` · ${rate(progress.bytes, progress.elapsed)}`}
                  </p>
                  {!progress.running && progress.total > 0 && (
                    <p className="settings__note">
                      {progress.total === 0
                        ? "Already in step — nothing to move."
                        : `Done. ${progress.done} of ${progress.total} moved from ${progress.peer}.`}
                    </p>
                  )}
                </>
              )}
            </div>
          )}

          {/*
        SYNC-006. The other way to sync, and the one that works when the two
        devices are never switched on at the same time — which for a laptop and
        a phone is most of the time. Playlists and tempo corrections go in a
        file beside the music, in the owner's own storage.
      */}
          <h3 className="label sync__heading">through your server</h3>
          <p className="settings__hint">
            Keeps playlists and tempo corrections in a file next to your music,
            so devices that are never on at the same time still agree. Track
            files are already there; this is only the part the app added.
          </p>
          <div className="settings__actions">
            <button
              className="settings__button"
              disabled={shared === "running"}
              onClick={() => void syncShared()}
            >
              {shared === "running" ? "Syncing…" : "Sync through the server"}
            </button>
          </div>
          {sharedNote && <p className="settings__note">{sharedNote}</p>}

          {note && <p className="settings__note">{note}</p>}
        </>
      )}
    </section>
  );
}

function rate(bytes: number, seconds: number): string {
  if (seconds <= 0 || bytes <= 0) return "—";
  const mb = bytes / 1_000_000 / seconds;
  return `${mb.toFixed(1)} MB/s`;
}
