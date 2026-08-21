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
 * connect, scan, analyse, in that order, each one showing what it did.
 * Appearance sits after them for the same reason: it is what someone comes
 * back to Settings for, not what got them here the first time.
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
import { YourData } from "./YourData";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { SyncPanel } from "../components/SyncPanel";
import { VaporMark, LOGO_POSE } from "../components/VaporMark";
import { HelpModal } from "../components/HelpModal";
import { SettingRow, SettingGroup } from "../components/SettingRow";
import { AppearanceControl } from "../components/Appearance";
// The notices themselves, not a second copy of them. Same reasoning as the
// Vibe help sheet importing `ai_dj_workflow.md`: a licence notice that is
// retyped is one that goes stale, and this one is an obligation rather than
// a convenience — CC BY wants attribution the user can actually see.
import notices from "../../../THIRD_PARTY_NOTICES.md?raw";

type Busy = "idle" | "saving" | "scanning" | "analysing";

/** Which card a notice belongs to, so an answer appears beside its question. */
type Card = "remote" | "analysis" | "data" | "dupes";

/**
 * Collapse a scheme typed on top of the prefilled one.
 *
 * The box starts at `https://` so the shape of the answer is visible before it
 * is typed. Someone pasting a whole address then lands on
 * `https://https://app.koofr.net`, which is what a prefix does when it is real
 * text rather than a placeholder — and pasting an address is the *normal* way
 * to fill this in, not an edge case.
 */
export function withoutDoubledScheme(value: string): string {
  return value.replace(/^(https?:\/\/)(?=https?:\/\/)/i, "");
}

export function Settings() {
  /** Whether the licences sheet is open. */
  const [licences, setLicences] = useState(false);
  /** How much of the library has been asked about, for the Fetch row. */
  const [looked, setLooked] = useState<core.LookupCounts | null>(null);

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

  /** The settings as the backend holds them.
   *
   *  The remote fields above are unpacked into their own state because they are
   *  edited before being saved. This is for the ones that are a switch: there
   *  is no draft of a checkbox, so it reads and writes the real value. */
  const [settings, setSettings] = useState<core.Settings | null>(null);

  const [status, setStatus] = useState<core.AnalysisStatus | null>(null);
  /** How many files are second-or-later copies, so the switch can say. */
  const [dupes, setDupes] = useState<number | null>(null);
  /** Whether a pass has been seen running since Analyse was pressed. */
  const [started, setStarted] = useState(false);
  const [progress, setProgress] = useState<core.AnalysisProgress | null>(null);

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

  // Cache size and data location used to be read here too, for a summary this
  // screen no longer draws: Your Data is at the bottom of it and reports both,
  // and asking the backend twice for one answer is how two numbers on one
  // screen end up disagreeing.
  const refresh = useCallback(async () => {
    const [s, d, l] = await Promise.allSettled([
      core.analysisStatus(),
      core.duplicateCount(),
      core.lookupCounts(),
    ]);
    if (s.status === "fulfilled") {
      setStatus(s.value);
      if (s.value.running) setStarted(true);
    }
    if (d.status === "fulfilled") setDupes(d.value);
    // Left alone on failure rather than zeroed — a row that reports "0 of 0"
    // because a call failed is the rails' bug in a different place.
    if (l.status === "fulfilled") setLooked(l.value);
  }, []);

  // The lookup row's subtitle counts what has been fetched, and lookups happen
  // as tracks load rather than on a press — so the count moves while somebody
  // listens, not only when this screen acts.
  useEffect(() => {
    const timer = setInterval(() => void refresh(), 4000);
    return () => clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    core
      .settings()
      .then((s) => {
        // Prefilled where nothing is configured yet, so the shape of the answer
        // is visible before it is typed. A placeholder cannot do this: it
        // vanishes the moment someone starts typing, which is exactly when the
        // "https://" or the leading "/dav/" would have been useful.
        setSettings(s);
        setUrl(s.remote.url || "https://");
        setUsername(s.remote.username);
        setFolder(s.remote.folder || "/dav/");
        void checkStored(s.remote.username);
      })
      // Loading the settings themselves failed, which belongs beside the
      // fields that could not be filled in.
      .catch((e: unknown) => setError({ card: "remote", text: messageOf(e) }));
    void refresh();
  }, [refresh, checkStored]);

  /*
   * Progress arrives per track rather than being polled: the pass is minutes
   * long — hours on a large library over a slow connection — and the backend
   * already emits an event for exactly this.
   *
   * `refresh` on every event, not only on the last one. `status` carries
   * `running`, and it was read once when this screen mounted and then not
   * again until a pass *finished*. So a pass that started afterwards left the
   * screen holding a snapshot that said nothing was running — which is what
   * gates the meter below, so a pass could be working through a library with
   * no sign of it on screen at all and a count that never moved.
   *
   * It is a local call answering from memory, once a track, so this is cheap.
   */
  useEffect(() => {
    const unlisten = listen<core.AnalysisProgress>("analysis-progress", (e) => {
      setProgress(e.payload);
      void refresh();
      if (e.payload.done >= e.payload.total) {
        setBusy("idle");
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  /*
   * A pass that is no longer running gives the buttons back.
   *
   * `busy` is set by pressing Analyse and was cleared only by a pass reaching
   * its last track. Anything else that ended it — a connection that died, the
   * credential going unreadable behind a locked screen — left this screen with
   * its own controls disabled for as long as the app stayed open, which reads
   * as the app having stopped working.
   */
  useEffect(() => {
    // `started` guards against the snapshot this screen mounted with: without
    // it, pressing Analyse is undone by a `running: false` read from before the
    // press, and the button comes straight back as though nothing happened.
    if (busy === "analysing" && started && status?.running === false) {
      setBusy("idle");
      setStarted(false);
    }
  }, [busy, started, status?.running]);

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
        // The library's contents are what a scan is for, so the reads
        // Library remembered are no longer answers.
        window.dispatchEvent(new Event("vapor:library-changed"));
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
    // Read the status straight back.
    //
    // The backend marks the pass running *before* it spawns the thread, so this
    // returns `running: true` and the meter appears at once rather than at the
    // first track — which on a slow connection is minutes away. It also stops
    // the release below from firing against the snapshot taken when this screen
    // mounted, which still says nothing is running.
    await refresh();
  }

  return (
    <div className="settings">
      <header className="settings__head">
        <h1 className="settings__title">Settings</h1>
      </header>

      <section className="settings__card glass">
        <h2 className="settings__section">Where your music lives</h2>

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
            onChange={(e) => setUrl(withoutDoubledScheme(e.target.value))}
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

      {/* After the storage card, before the library rows: connecting storage
          is what a first run is for, and this is what someone comes back for. */}
      <SettingGroup title="appearance">
        <AppearanceControl />
      </SettingGroup>

      <SettingGroup title="library">
        {/*
          One row, not a card of caveats.

          The card explained that every track has to be fetched before it can be
          listened to, that the download dominates, and that hundreds of tracks
          take hours — all true, and all of it read as discouragement rather than
          as information. The progress line says the same thing while it happens,
          which is when it matters.
        */}
        <SettingRow
          title="Analyse"
          subtitle={
            status?.running
              ? `${(progress?.done ?? status.analysed).toLocaleString()} of ${(
                  progress?.total ?? status.total
                ).toLocaleString()} — ${status.current || "working…"}`
              : status
                ? `${status.analysed.toLocaleString()} of ${status.total.toLocaleString()} done — let Vibe DJ find tempo, key and cue points, and more`
                : "Let Vibe DJ find tempo, key and cue points, and more"
          }
        >
          {/*
            One button that changes what it does, rather than two.

            "Make it just one button" cannot mean losing the ability to stop:
            a library-wide pass runs for hours, and a run you cannot call off
            is worse than a second button. So the row's single control starts
            the pass and stops it — which is also how the transport's play
            control behaves, and for the same reason.
          */}
          {status?.running ? (
            <button
              className="settings__rowbutton settings__rowbutton--quiet"
              onClick={() => {
                void core.cancelAnalysis().finally(() => void refresh());
              }}
            >
              Stop
            </button>
          ) : (
            <button
              className="settings__rowbutton"
              disabled={busy === "analysing"}
              onClick={() => void analyse()}
            >
              Analyse
            </button>
          )}
        </SettingRow>

        {error?.card === "analysis" && (
          <ErrorNotice
            error={error.text}
            onDismiss={() => setError(null)}
          />
        )}
      </SettingGroup>

      <SyncPanel />

      <SettingGroup title="library">
        {/*
          Hides, never deletes.

          These are the person's own files. A library quietly showing fewer
          tracks than are on disk is one you cannot trust to tell you what you
          have, so the copies stay where they are, to be tidied by hand.

          The Vibe DJ excludes duplicates whatever this says: two copies of one
          track share a tempo, key and intensity, so a set free to use both
          mixes a record into itself.
        */}
        <SettingRow
          title="Hide duplicates"
          subtitle={
            dupes === null
              ? "Counting…"
              : dupes === 0
                ? "Nothing is in your library twice"
                : settings?.hideDuplicates
                  ? `${dupes.toLocaleString()} hidden`
                  : `${dupes.toLocaleString()} extra ${dupes === 1 ? "copy" : "copies"} — still showing`
          }
        >
          <label className="settings__switch settings__switch--bare">
            <input
              type="checkbox"
              aria-label="Hide duplicates"
              checked={settings?.hideDuplicates ?? false}
              disabled={!settings}
              onChange={(e) => {
                void core
                  .setHideDuplicates(e.target.checked)
                  .then(setSettings)
                  .catch((err: unknown) =>
                    setNote({ card: "dupes", text: messageOf(err) }),
                  );
              }}
            />
          </label>
        </SettingRow>

        {/*
          One switch, one pass — they used to be two features that looked like
          one. Lyrics were fetched per track on demand and artwork in a
          library-wide pass, so "look things up" meant different work depending
          on where you pressed it. The subtitle counts tracks that have been
          *asked about*, which is what decides whether pressing again does
          anything: a track LRCLIB has never heard of has still been asked.
        */}
        <SettingRow
          title="Fetch lyrics and artwork"
          subtitle={
            !settings?.metadataLookupEnabled
              ? "Off — nothing about your library leaves this device"
              : looked === null
                ? "On — fetched as you play"
                : `${looked.fetched.toLocaleString()} of ${looked.total.toLocaleString()} fetched, as you play`
          }
        >
          <label className="settings__switch settings__switch--bare">
            <input
              type="checkbox"
              aria-label="Allow looking things up"
              checked={settings?.metadataLookupEnabled ?? false}
              disabled={!settings}
              onChange={(e) => {
                void core
                  .setMetadataLookup(e.target.checked)
                  .then(setSettings)
                  .catch((err: unknown) =>
                    setNote({ card: "data", text: messageOf(err) }),
                  );
              }}
            />
          </label>

        </SettingRow>

        {note?.card === "dupes" && (
          <p className="settings__note">{note.text}</p>
        )}
        {note?.card === "data" && <p className="settings__note">{note.text}</p>}
      </SettingGroup>

      <section className="settings__card glass">
        <h2 className="settings__section">About</h2>
        {/*
          The logo, pinned rather than animated. `LOGO_POSE` is the same frame
          the app icon is rendered from, so this is not a picture *of* the mark
          — it is the mark, drawn by the same code at the same pose. If the icon
          is ever repinned, this moves with it.
        */}
        <div className="settings__lockup">
          <VaporMark size={96} pose={LOGO_POSE} />
          <div className="settings__lockup-text">
            <span className="settings__lockup-name">Vapor Music</span>
            <span className="label">Music, continuous</span>
          </div>
        </div>
        {/*
          One line, not three. What has to survive the trim is what is owed or
          claimed: the copyright, the CC BY attribution (which the licence
          wants where users can see it, so it cannot move behind the button),
          and the on-device claim, which is the whole privacy position. The
          component credits went to the sheet, where they already were.
        */}
        <p className="settings__hint">
          © 2026 Dylan Growcoot, all rights reserved. Tempo and key are worked
          out on this device. Icons by Gregor Cresnar, from the Noun Project,
          under a{" "}
          <a
            href="https://creativecommons.org/licenses/by/3.0/"
            target="_blank"
            rel="noreferrer"
          >
            Creative Commons Attribution licence
          </a>
          .
        </p>
        {/* The full notices, in the app rather than only in the repository.
            CC BY requires attribution visible to the people using the work,
            and a file on GitHub does not reach them. It is also where the
            MPL-2.0 notice for Symphonia belongs. */}
        <button className="settings__licences" onClick={() => setLicences(true)}>
          Licences and attributions
        </button>
      </section>

      {/*
        Your Data, at the bottom of Settings rather than on a tab of its own.
        It is still the screen the sovereignty claim is proved on — see
        design/README.md — and it reads as the end of "here is how this is set
        up", which is where someone asking "what does it keep, and where" is
        already standing.
      */}
      <YourData embedded />

      {/* The full notices, rendered from the file itself. */}
      {licences && (
        <HelpModal
          title="Licences and attributions"
          markdown={notices}
          onClose={() => setLicences(false)}
        />
      )}
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

