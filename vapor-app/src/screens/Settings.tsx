/**
 * Settings — where a library gets connected.
 *
 * This screen is the reason the app was not usable. Everything behind it has
 * worked for several commits: the scan walks WebDAV, the analyser fills in
 * tempo and key, the cache fetches on demand, the engine plays. None of it was
 * reachable, because nothing could tell the app where the music lives — the
 * Library screen's own empty state said "Connect your storage in Settings" and
 * there was no Settings.
 *
 * So the shape here follows the first run rather than the design's grouping:
 * connect, scan, analyse, in that order, each one showing what it did. The
 * appearance and theme controls the design also specifies are not here yet —
 * they change how the app looks, not whether it works.
 */

import {
  cloneElement,
  isValidElement,
  useCallback,
  useEffect,
  useId,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import * as core from "../lib/core";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";

type Busy = "idle" | "saving" | "scanning" | "analysing";

/** Which card a notice belongs to, so an answer appears beside its question. */
type Card = "remote" | "analysis" | "data";

export function Settings() {
  const [url, setUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [folder, setFolder] = useState("Music");

  const [busy, setBusy] = useState<Busy>("idle");
  const [error, setError] = useState<{ card: Card; text: string } | null>(null);
  /** Which card an action's answer belongs beside.
   *
   *  Previously one notice rendered after every card, so clicking "Scan
   *  library" at the top of the page put the result four cards below the fold:
   *  the button appeared to do nothing until you scrolled. An answer belongs
   *  next to the thing that asked the question. */
  const [note, setNote] = useState<{ card: Card; text: string } | null>(null);

  const [status, setStatus] = useState<core.AnalysisStatus | null>(null);
  const [progress, setProgress] = useState<core.AnalysisProgress | null>(null);
  const [cache, setCache] = useState<core.CacheStatus | null>(null);
  const [location, setLocation] = useState("");

  /** Whether the keychain holds a password for the username in the box.
   *
   *  The box is always empty on load — the password is never read back into the
   *  UI — so without this the placeholder said "unchanged" whether or not
   *  anything was stored. Someone whose save had silently failed came back to a
   *  box claiming a credential existed, with no way to tell. */
  const [stored, setStored] = useState<boolean | null>(null);

  const configured = url.trim() !== "" && username.trim() !== "";

  /** Ask the keychain whether this account has a password. Debounced by the
   *  username changing rather than per keystroke — it is a keychain read. */
  const checkStored = useCallback(async (name: string): Promise<boolean | null> => {
    if (!name.trim()) {
      setStored(null);
      return null;
    }
    try {
      const answer = await core.hasWebdavPassword(name);
      setStored(answer);
      // Returned as well as stored, so a caller can report the answer without
      // racing the state update it just queued.
      return answer;
    } catch {
      // Unknown rather than false: claiming "none saved" because the check
      // failed would be the same lie in the other direction.
      setStored(null);
      return null;
    }
  }, []);

  const refresh = useCallback(async () => {
    const [s, c, l] = await Promise.allSettled([
      core.analysisStatus(),
      core.cacheStatus(),
      core.dataLocation(),
    ]);
    if (s.status === "fulfilled") setStatus(s.value);
    if (c.status === "fulfilled") setCache(c.value);
    if (l.status === "fulfilled") setLocation(l.value);
  }, []);

  useEffect(() => {
    core
      .settings()
      .then((s) => {
        setUrl(s.remote.url);
        setUsername(s.remote.username);
        setFolder(s.remote.folder);
        void checkStored(s.remote.username);
      })
      // Loading the settings themselves failed, which belongs beside the
      // fields that could not be filled in.
      .catch((e: unknown) => setError({ card: "remote", text: messageOf(e) }));
    void refresh();
  }, [refresh, checkStored]);

  // Progress arrives per track rather than being polled: the pass is minutes
  // long and the backend already emits an event for exactly this.
  useEffect(() => {
    const unlisten = listen<core.AnalysisProgress>("analysis-progress", (e) => {
      setProgress(e.payload);
      if (e.payload.done >= e.payload.total) {
        setBusy("idle");
        void refresh();
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  async function run<T>(
    kind: Busy,
    card: Card,
    fn: () => Promise<T>,
    done: (v: T) => void,
  ) {
    setBusy(kind);
    setError(null);
    setNote(null);
    try {
      done(await fn());
    } catch (e: unknown) {
      setError({ card, text: messageOf(e) });
    } finally {
      // Analysis clears its own flag when the last track lands, since it
      // outlives the call that started it.
      if (kind !== "analysing") setBusy("idle");
    }
  }

  /**
   * Write the form to the backend. Shared by Save and Scan — see `scan`.
   *
   * Returns what was *observed* afterwards rather than nothing, so the caller
   * reports a verified outcome instead of the absence of an exception. Saving
   * a password used to be followed by "Saved. The password is in your
   * keychain" unconditionally — and for the whole life of the project the
   * keychain was a mock that stored nothing (TD-50), so the notice was a lie
   * printed in green directly above a red badge saying the opposite.
   */
  async function apply(): Promise<{ submitted: boolean; stored: boolean | null }> {
    await core.setRemoteConfig(url, username, folder);

    // An empty box means "keep the password already stored", so an accidental
    // save does not wipe a working credential.
    const submitted = password !== "";
    if (submitted) {
      await core.saveWebdavPassword(username, password);
      setPassword("");
    }

    // Ask the backend what is actually there now. This is the check the
    // success notice is allowed to speak from.
    const confirmed = await checkStored(username);
    return { submitted, stored: confirmed };
  }

  async function save() {
    await run("saving", "remote", apply, ({ submitted, stored: confirmed }) => {
      // A password was typed and the keychain does not have it. The call
      // returned without throwing, and it still failed — which is precisely
      // the case that used to render as success.
      if (submitted && confirmed === false) {
        setError({
          card: "remote",
          text:
            "The server details were saved, but the password did not reach " +
            "your keychain. Nothing that needs the server will work until it " +
            "does. If your system asked for permission to use the keychain " +
            "and it was denied, allow it and press Save again.",
        });
        return;
      }

      if (submitted && confirmed === null) {
        setNote({
          card: "remote",
          text:
            "Server details saved. The keychain could not be checked, so " +
            "whether the password stored is unknown — press Scan to find out.",
        });
        return;
      }

      setNote({
        card: "remote",
        text: confirmed
          ? "Saved. The password is in your keychain, not in a file."
          : "Saved. No password is stored yet — type one above and press Save.",
      });
    });
  }

  async function scan() {
    await run(
      "scanning",
      "remote",
      async () => {
        // Apply the form first.
        //
        // Scanning reads the credential from the keychain, so a password typed
        // but not saved is a password the scan cannot see — and the failure it
        // produced said "no password saved" while one was plainly visible in
        // the box above it. Pressing Scan after filling the form means "use
        // what I just typed"; making that a two-step ritual serves nothing.
        await apply();
        return core.scanLibrary();
      },
      (report) => {
      // A folder that is not there is now an error with the path in it
      // (TD-49), so reaching here means the path was real. Zero tracks
      // therefore means the folder holds no audio, which is a different
      // problem and gets a different sentence.
      const skipped =
        report.unreadable > 0
          ? ` ${report.unreadable.toLocaleString()} ` +
            `${report.unreadable === 1 ? "folder" : "folders"} could not be ` +
            `read and ${report.unreadable === 1 ? "was" : "were"} skipped.`
          : "";

      setNote({
        card: "remote",
        text:
          report.tracks === 0
            ? `That folder is there, but no audio files are in it or below it ` +
              `(${report.directories.toLocaleString()} folders searched). ` +
              `Check the folder path points at your music.${skipped}`
            : `Found ${report.tracks.toLocaleString()} tracks in ` +
              `${report.directories.toLocaleString()} folders.${skipped}`,
        });
        void refresh();
      },
    );
  }

  async function analyse() {
    setProgress(null);
    await run("analysing", "analysis", core.analyseLibrary, () => {});
  }

  return (
    <div className="settings">
      <header className="settings__head">
        <h1 className="settings__title">Settings</h1>
      </header>

      <section className="settings__card glass">
        <h2 className="settings__section">Where your music lives</h2>
        <p className="settings__hint">
          A WebDAV address — Nextcloud, Koofr, Proton Drive or your own server.
          Vapor reads it; nothing is uploaded anywhere.
        </p>

        <Field
          label="Server address"
          hint="Your storage provider's WebDAV address. It starts with https://."
        >
          <input
            className="settings__input"
            type="url"
            value={url}
            placeholder="https://example.com/dav"
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setUrl(e.target.value)}
          />
        </Field>

        <Field
          label="Username"
          hint="Almost always the email address you sign in with."
        >
          <input
            className="settings__input"
            type="text"
            value={username}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setUsername(e.target.value)}
            onBlur={() => void checkStored(username)}
          />
        </Field>

        <Field
          label="Password"
          hint="Usually an app password generated by your provider, not your account password."
          note={
            /* Three states, because there are three.
             *
             * This used to be a two-way branch whose `else` claimed "stored in
             * your keychain" for everything that was not a confirmed absence —
             * including "the check failed" and, worse, "you are typing a
             * password right now". Typing is not saving, so the badge turned
             * green the moment a character was entered and asserted a
             * credential that did not exist. It said the same thing while the
             * keychain was a mock store that had never held anything (TD-50).
             *
             * A green badge here now means one thing: `has_webdav_password`
             * was asked and said yes. */
            stored === true ? (
              <span className="settings__field-note sovereign">
                Stored in your operating system's keychain
              </span>
            ) : stored === false ? (
              <span className="settings__field-note settings__field-note--warn">
                {password ? "Not saved yet — press Save" : "No password saved yet"}
              </span>
            ) : (
              // Unknown. Not "stored" and not "none": the question could not
              // be asked, and saying either would be a guess with a colour on
              // it.
              <span className="settings__field-note">
                Could not check the keychain
              </span>
            )
          }
        >
          <input
            className="settings__input"
            type="password"
            value={password}
            placeholder={stored ? "unchanged" : "required"}
            autoComplete="off"
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>

        <Field
          label="Folder"
          hint="The full path to your music on that server, not just the folder's name."
        >
          <input
            className="settings__input"
            type="text"
            value={folder}
            placeholder="Music"
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setFolder(e.target.value)}
          />
        </Field>

        <div className="settings__actions">
          <button
            className="settings__button settings__button--primary"
            onClick={() => void save()}
            disabled={busy !== "idle" || !configured}
          >
            {busy === "saving" ? "Saving…" : "Save"}
          </button>
          <button
            className="settings__button"
            onClick={() => void scan()}
            disabled={busy !== "idle" || !configured}
          >
            {busy === "scanning" ? "Scanning…" : "Scan library"}
          </button>
        </div>

        {error?.card === "remote" && (
          <ErrorNotice error={error.text} onDismiss={() => setError(null)} />
        )}
        {note?.card === "remote" && (
          <p className="settings__note">{note.text}</p>
        )}
      </section>

      <section className="settings__card glass">
        <h2 className="settings__section">Listening to your library</h2>
        <p className="settings__hint">
          Tempo, key and cue points, worked out on this device. About half a
          second a track, and only once — the results are kept.
        </p>

        {status && (
          <p className="settings__stat numeric">
            {status.analysed.toLocaleString()} of{" "}
            {status.total.toLocaleString()} analysed
          </p>
        )}

        {busy === "analysing" && progress && (
          <>
            <div
              className="settings__meter"
              role="progressbar"
              aria-valuenow={progress.done}
              aria-valuemin={0}
              aria-valuemax={progress.total}
            >
              <div
                className="settings__meter-fill"
                style={{
                  width: `${(progress.done / Math.max(progress.total, 1)) * 100}%`,
                }}
              />
            </div>
            <p className="settings__stat numeric">
              {/* The design's Loading screen words it exactly this way. */}
              Listening… {progress.done} of {progress.total} · on this device
            </p>
          </>
        )}

        <div className="settings__actions">
          <button
            className="settings__button"
            onClick={() => void analyse()}
            disabled={busy !== "idle" || !status?.total}
          >
            Analyse
          </button>
          {busy === "analysing" && (
            <button
              className="settings__button"
              onClick={() => {
                void core.cancelAnalysis();
                setBusy("idle");
              }}
            >
              Stop
            </button>
          )}
        </div>

        {error?.card === "analysis" && (
          <ErrorNotice error={error.text} onDismiss={() => setError(null)} />
        )}
        {note?.card === "analysis" && (
          <p className="settings__note">{note.text}</p>
        )}
      </section>

      <section className="settings__card glass">
        <h2 className="settings__section">Your data</h2>
        {cache && (
          <>
            <p className="settings__stat numeric">
              {formatBytes(cache.bytes)} of {formatBytes(cache.maxBytes)} used ·{" "}
              {cache.tracksCached.toLocaleString()} of{" "}
              {cache.tracksTotal.toLocaleString()} tracks on this device
            </p>
            <p className="settings__path numeric" title={cache.location}>
              {location || cache.location}
            </p>
          </>
        )}
        <p className="settings__hint">
          Everything is a plain file here. No account, nothing shared, nothing
          leaves unless you move it.
        </p>
        <div className="settings__actions">
          <button
            className="settings__button settings__button--danger"
            onClick={() => {
              // Irreversible and outward-looking enough to deserve a question,
              // and the wording says what actually goes.
              if (
                !window.confirm(
                  "Delete the local index, analysis, playlists, cached audio and the saved password? Your music on the server is untouched.",
                )
              ) {
                return;
              }
              void core.deleteAllData().then(refresh);
            }}
            disabled={busy !== "idle"}
          >
            Delete everything stored here
          </button>
        </div>
      </section>

    </div>
  );
}

/**
 * A labelled input, with optional help behind an (i).
 *
 * The label is a real `<label htmlFor>` tied to the input, so the field has a
 * name in the accessibility tree and clicking the text focuses the box. An
 * earlier version put the (i) beside the text and turned the wrapper into a
 * `<div>`, which silently removed that association from every field on the
 * screen — the button needs to sit outside the label (clicking help should not
 * focus the input), so the association has to be explicit rather than implied
 * by nesting.
 *
 * The id is injected into the child rather than demanded from every caller:
 * there is exactly one input per field, and threading a matching pair of ids
 * through four call sites is four chances to typo one.
 *
 * Help is a button rather than a `title=` tooltip: `title` needs a hovering
 * pointer, so it does not exist on touch and cannot be reached from a
 * keyboard. Collapsed by default, because four open hints bury a four-field
 * form.
 */
function Field({
  label,
  hint,
  note,
  children,
}: {
  label: string;
  hint?: string;
  /** Rendered under the control. A separate slot rather than a second child,
   *  because the id below is injected into `children` and an array of children
   *  has no single control to inject it into — which silently cost the password
   *  field its label. */
  note?: React.ReactNode;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const hintId = `${id}-hint`;

  const described = hint && open ? { "aria-describedby": hintId } : {};
  const input = isValidElement(children)
    ? cloneElement(
        children as React.ReactElement<{
          id?: string;
          "aria-describedby"?: string;
        }>,
        { id, ...described },
      )
    : children;

  return (
    <div className="settings__field">
      <div className="settings__field-label">
        <label className="label" htmlFor={id}>
          {label}
        </label>
        {hint && (
          <button
            type="button"
            className="settings__hint-toggle"
            aria-expanded={open}
            aria-controls={hintId}
            aria-label={`What goes in ${label}`}
            onClick={() => setOpen((o) => !o)}
          >
            i
          </button>
        )}
      </div>
      {input}
      {note}
      {hint && open && (
        <p className="settings__field-hint" id={hintId}>
          {hint}
        </p>
      )}
    </div>
  );
}

/** Bytes as a person reads them. Binary units, since that is what the bound
 *  is expressed in. */
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const value = bytes / 1024 ** i;
  return `${value >= 10 || i === 0 ? Math.round(value) : value.toFixed(1)} ${units[i]}`;
}
