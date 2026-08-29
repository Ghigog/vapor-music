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
import { useCallback, useEffect, useRef, useState } from "react";
import * as core from "../lib/core";
import { SettingRow } from "./SettingRow";
import { Confirm } from "./Confirm";
import { ErrorNotice, messageOf } from "./ErrorNotice";

export function SyncPanel() {
  const [view, setView] = useState<core.SyncView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  /** The peer whose code is being typed here, if any. */
  const [entering, setEntering] = useState<string | null>(null);
  const [code, setCode] = useState("");
  /** Everything moves. Kept as a value because `syncWith` takes it, not as a
   *  choice, because it was never one anybody made. */
  const what: core.SyncWhat = { tracks: true, playlists: true };
  const [shared, setShared] = useState<"idle" | "running">("idle");
  /** The device a "stop syncing with this?" question is open about. */
  const [forgetting, setForgetting] = useState<core.TrustedDevice | null>(null);
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

  /*
   * The server round trip, run rather than offered.
   *
   * It used to be a button under two paragraphs distinguishing it from the
   * over-the-network kind. The distinction is real — this one works when the
   * two devices are never awake together, which for a laptop and a phone is
   * most of the time — but it is the app's distinction, not the owner's, and
   * making somebody read it before pressing anything is how playlists stayed
   * out of step on the device that was switched off.
   *
   * Once per switch-on rather than on a timer: it is a WebDAV round trip, and
   * `sync()` runs it again after every device sync, which covers the case
   * where something actually changed.
   */
  const started = useRef(false);
  useEffect(() => {
    if (!view?.enabled || started.current) return;
    started.current = true;
    void syncShared();
    // `syncShared` is stable for the life of the panel and re-running this on
    // every render is the opposite of once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view?.enabled]);

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
      // The two halves are one action now, so the file beside the music is
      // brought into step with whatever just arrived over the network.
      await syncShared();
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
        // Reported in the same breath as the arrivals, and deliberately not
        // softened. A playlist disappearing without being told why is the
        // thing that makes a sync feel unsafe, and it was deleted on purpose
        // somewhere.
        r.playlistsDeleted &&
          `${r.playlistsDeleted} playlist${r.playlistsDeleted === 1 ? " was" : "s were"} deleted elsewhere and removed here`,
        r.foldersDeleted &&
          `${r.foldersDeleted} folder${r.foldersDeleted === 1 ? " was" : "s were"} deleted elsewhere and removed here`,
      ].filter(Boolean);

      // Four outcomes, and three of them look identical if only the changes
      // are reported: nothing there yet, nothing to change, and a failure that
      // did not throw.
      //
      // Two or three words each, because this line appears unheaded after
      // something the owner did not press. "Already in step. Yours is on the
      // server too." was the no-change case, and it borrowed the "too" from
      // the case above it, where something had in fact arrived — here it
      // pointed at nothing. A status has to be readable without knowing the
      // mechanism that produced it. The change list stays a sentence because
      // it is information rather than status, and the deletions in it are the
      // part somebody most needs to read.
      setSharedNote(
        r.created
          ? "Copied to server"
          : changes.length > 0
            ? `${changes.join(", ")}.`
            : "Up to date",
      );
      window.dispatchEvent(new Event("vapor:playlists-changed"));
    } catch (e: unknown) {
      setError(messageOf(e));
    } finally {
      setShared("idle");
    }
  }

  async function forget(peer: core.TrustedDevice) {
    setForgetting(null);
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

  /*
   * Who this is sharing with, as a sentence rather than a list of cards.
   *
   * The panel opened with three paragraphs about how sync works and what it
   * does not do, then the switch, then everything else. What a person wants
   * from a glance is whether it is on and who it reaches; the rest belongs
   * below, and only once it is on.
   */
  return (
    /* No card and no heading of its own.

       Sharing is a library setting and lives as a row in the Library group
       (POL-5). Drawing a `settings__card` and a `SettingGroup title="network"`
       here put a second heading inside another section's rows, which is what
       `.settings__shared` in settings.css used to unpick with `display: none`
       — and that hid the heading from the accessibility tree as well as from
       sight. There is no heading to hide now. */
    <>
        {/*
          One switch, where there were two.

          A `SettingRow` here and a bare checkbox below both called
          `setSyncEnabled` and both read `view.enabled` — the same setting drawn
          twice, so each one moved the other and neither was the control.

          Named for what it does rather than for the network it does it on: the
          Wi-Fi name is not obtainable without a location permission on either
          platform, and this machine is on Ethernet besides, where there is no
          Wi-Fi name to print. See the commit for the detail.
        */}
        <SettingRow
          title="Share across this network"
          subtitle={
            view.enabled
              ? `This device appears as ${view.deviceName}`
              : "Off — nothing is announced and nothing is listening"
          }
        >
          <label className="settings__switch settings__switch--bare">
            <input
              type="checkbox"
              aria-label="Share across this network"
              checked={view.enabled}
              onChange={(e) => {
                const on = e.target.checked;
                // Switching off forgets every pairing, which is a consequence
                // worth stating rather than leaving somebody to discover.
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
          </label>
        </SettingRow>

      {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}

      {/* Outside the `enabled` block below, because the note that matters most
          is the one about switching *off* — and by the time it is written,
          everything inside that block is gone. */}
      {!view.enabled && note && <p className="settings__note">{note}</p>}

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
              {/* The row above already says who this shares with, so this says
                  the thing that row cannot: what to do about it. */}
              Pair one below, and it stays paired until you say otherwise.
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
                      onClick={() => setForgetting(peer)}
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

          {/* No "what moves" checkboxes. Both were on by default and nobody
              turned them off — a pair of switches that only ever say yes is
              furniture. A paired device gets tracks and playlists. */}

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
                  {/* No nothing-to-move branch: the block above only renders
                      when `progress.total > 0`, so the zero case was
                      unreachable. A device sync that moves nothing still
                      reports through the shared-document note below. */}
                  {!progress.running && progress.total > 0 && (
                    <p className="settings__note">
                      {`Done. ${progress.done} of ${progress.total} moved from ${progress.peer}.`}
                    </p>
                  )}
                </>
              )}
            </div>
          )}

          {/*
        SYNC-006 runs on its own now, from the effect above — it was a button
        called "Sync through the server" under two paragraphs explaining a
        distinction nobody has to care about. Playlists and tempo corrections
        go in a file beside the music either way; whether they travelled over
        the network or through the server is the app's problem.

        Still reported while it happens, because it touches the server and a
        thing that touches the network in the background should be visible.
      */}
          {shared === "running" && (
            <p className="settings__note">Syncing…</p>
          )}
          {sharedNote && <p className="settings__note">{sharedNote}</p>}

          {note && <p className="settings__note">{note}</p>}
        </>
      )}

      {forgetting && (
        <Confirm
          title={`Stop syncing with ${forgetting.name}?`}
          body={`It will have to be paired again, with a new code. Nothing in either library is deleted.`}
          confirmLabel="Forget"
          onConfirm={() => void forget(forgetting)}
          onCancel={() => setForgetting(null)}
        />
      )}
    </>
  );
}

function rate(bytes: number, seconds: number): string {
  if (seconds <= 0 || bytes <= 0) return "—";
  const mb = bytes / 1_000_000 / seconds;
  return `${mb.toFixed(1)} MB/s`;
}
