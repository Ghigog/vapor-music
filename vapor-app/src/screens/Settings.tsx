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
import { open } from "@tauri-apps/plugin-dialog";
import * as core from "../lib/core";
import { YourData } from "./YourData";
import { SupporterPins } from "../components/SupporterPins";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { SyncPanel } from "../components/SyncPanel";
import { VaporMark } from "../components/VaporMark";
import { HelpModal } from "../components/HelpModal";
import { AnalysisFailures } from "../components/AnalysisFailures";
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

/**
 * A storage provider, by the name somebody types into the address box.
 *
 * ## Nothing is in here that was not read off the provider's own documentation
 *
 * Every address and path below was checked against the provider's own help
 * pages on 2026-08-28. That is the whole reason the list is short: a
 * suggestion that fills the field with an address which cannot work is worse
 * than no suggestion, because it moves the failure from "I do not know what to
 * type" to "I typed what it told me and it is broken".
 *
 * ## `address: null` is an answer, not a gap
 *
 * The three providers people reach for first — Google Drive, Proton Drive,
 * Dropbox — have no WebDAV at all. Staying silent about those leaves someone
 * hunting for an address that does not exist, so the list says so instead.
 * Anything not named here gets silence, which is honest: it means this app
 * does not know, not that the provider has nothing.
 */
type Provider = {
  /** As printed. */
  name: string;
  /** What somebody might type. Matched as a prefix. */
  typed: string[];
  /** The verified WebDAV address, or null where the provider offers none. */
  address: string | null;
  /** Where that account's own files start, where the provider fixes it. */
  folder?: string;
};

const PROVIDERS: Provider[] = [
  {
    name: "Koofr",
    typed: ["koofr"],
    address: "https://app.koofr.net",
    folder: "/dav/Koofr/",
  },
  // Two data regions, two hosts, and the account decides which — so both are
  // offered rather than one guessed at.
  {
    name: "pCloud (Europe)",
    typed: ["pcloud"],
    address: "https://ewebdav.pcloud.com",
    folder: "/",
  },
  {
    name: "pCloud (US)",
    typed: ["pcloud"],
    address: "https://webdav.pcloud.com",
    folder: "/",
  },
  // `myfiles` rather than `webdav`: the same page documents both, and this one
  // is rooted in the account's own files instead of a directory above them.
  {
    name: "Fastmail",
    typed: ["fastmail"],
    address: "https://myfiles.fastmail.com",
    folder: "/",
  },
  {
    name: "Yandex Disk",
    typed: ["yandex"],
    address: "https://webdav.yandex.ru",
    folder: "/",
  },
  { name: "Google Drive", typed: ["googledrive", "gdrive"], address: null },
  { name: "Proton Drive", typed: ["proton", "protondrive"], address: null },
  { name: "Dropbox", typed: ["dropbox"], address: null },
];

/**
 * Providers matching what is in the address box, while it is still a name.
 *
 * A dot or a slash means the address is being typed rather than asked for, so
 * suggestions stop — otherwise a pasted `https://app.koofr.net` would sit
 * under a button offering to type it again.
 */
export function providersFor(value: string): Provider[] {
  const rest = value.replace(/^https?:\/\//i, "").trim();
  if (rest.length < 2 || rest.includes(".") || rest.includes("/")) return [];
  const word = rest.replace(/[\s\-_]/g, "").toLowerCase();
  return PROVIDERS.filter((p) => p.typed.some((t) => t.startsWith(word)));
}

/** The provider an address belongs to, so its folder can be offered. */
export function providerAt(value: string): Provider | undefined {
  const host = value.replace(/^https?:\/\//i, "").split("/")[0]?.toLowerCase();
  if (!host) return undefined;
  return PROVIDERS.find(
    (p) => p.address && p.address.replace(/^https?:\/\//i, "") === host,
  );
}

/**
 * Folders on this device the library reads from.
 *
 * Additive to the server above it, not an alternative — a person can have
 * both, and the music already on the laptop and the music on the NAS are one
 * library from where they are standing.
 *
 * Removing forgets where to look. It does not delete anything, and says so:
 * these are somebody's own files, and a settings screen that might be deleting
 * music is one nobody will touch twice.
 */
function useLocalFolders() {
  const [folders, setFolders] = useState<core.LocalFolder[]>([]);
  const [busy, setBusy] = useState(false);
  /* Kept apart, because the button that adds and the buttons that forget are
   * no longer on the same card (POL-5) — one shared string would have printed
   * a failed add underneath the list, a section away from the press. */
  const [addProblem, setAddProblem] = useState("");
  const [forgetProblem, setForgetProblem] = useState("");

  useEffect(() => {
    core
      .localFolders()
      .then(setFolders)
      .catch(() => {});
  }, []);

  async function add() {
    setAddProblem("");
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      setFolders(await core.addLocalFolder(picked));
      // Scanned here rather than left for the person to remember. A folder in
      // the list whose tracks are not in the library is a lie the screen tells.
      await core.scanLibrary();
    } catch (e) {
      setAddProblem(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function forget(id: string) {
    setForgetProblem("");
    setBusy(true);
    try {
      setFolders(await core.removeLocalFolder(id));
      await core.scanLibrary();
    } catch (e) {
      setForgetProblem(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return { folders, busy, addProblem, forgetProblem, add, forget };
}

/** What is on this device, listed. Adding one is up in "Where your music
 *  lives", which is the question this card answers the other half of. */
function FoldersCard({
  folders,
  busy,
  problem,
  forget,
}: {
  folders: core.LocalFolder[];
  busy: boolean;
  problem: string;
  forget: (id: string) => void;
}) {
  return (
    <section className="settings__card glass">
      <h2 className="settings__section">Music on this device</h2>

      {folders.length === 0 ? (
        <p className="settings__hint">No folders yet.</p>
      ) : (
        <ul className="folders">
          {folders.map((folder) => (
            <li key={folder.id} className="folders__row">
              <span className="folders__path" title={folder.path}>
                {folder.path}
              </span>
              <button
                type="button"
                className="settings__button"
                onClick={() => forget(folder.id)}
                disabled={busy}
              >
                Forget
              </button>
            </li>
          ))}
        </ul>
      )}

      {problem && <p className="settings__error">{problem}</p>}

      <p className="settings__hint">
        Forgetting a folder removes it from the library. Your files are not
        touched.
      </p>
    </section>
  );
}

export function Settings() {
  /** Whether the licences sheet is open. */
  const [licences, setLicences] = useState(false);
  /** Folders on this device. Lifted out of the card below, because the button
   *  that adds one now sits in a different section from the list of them. */
  const local = useLocalFolders();
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
  /** Tracks analysis could not describe. Empty is the ordinary case. */
  const [failures, setFailures] = useState<core.AnalysisFailure[]>([]);
  /** Whether the list of them is open. */
  const [showFailures, setShowFailures] = useState(false);
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
  const checkStored = useCallback(
    async (name: string): Promise<boolean | null> => {
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
    },
    [],
  );

  // Cache size and data location used to be read here too, for a summary this
  // screen no longer draws: Your Data is at the bottom of it and reports both,
  // and asking the backend twice for one answer is how two numbers on one
  // screen end up disagreeing.
  const refresh = useCallback(async () => {
    const [s, d, l, f] = await Promise.allSettled([
      core.analysisStatus(),
      core.duplicateCount(),
      core.lookupCounts(),
      core.analysisFailures(),
    ]);
    if (s.status === "fulfilled") {
      setStatus(s.value);
      if (s.value.running) setStarted(true);
    }
    if (d.status === "fulfilled") setDupes(d.value);
    // Left alone on failure rather than zeroed — a row that reports "0 of 0"
    // because a call failed is the rails' bug in a different place.
    if (l.status === "fulfilled") setLooked(l.value);
    // Same reasoning as the lookup counts: left alone on failure rather than
    // emptied, since "no problems" and "could not ask" are different answers
    // and only one of them should hide the line.
    if (f.status === "fulfilled") setFailures(f.value);
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
  async function apply(): Promise<{
    submitted: boolean;
    stored: boolean | null;
  }> {
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

      {/*
        One question — where is the music — answered in one card, cheapest
        answer first (POL-5). "Add a folder" needs no server, no address and no
        password, so it is the first control on the screen; the WebDAV form is
        the alternative, and reads as one.

        The list of folders already added is the card below this, because a
        list is not a control and putting it above the form pushed the form off
        a first run's screen.
      */}
      <section className="settings__card glass">
        <h2 className="settings__section">Where your music lives</h2>

        <div className="settings__actions">
          <button
            type="button"
            className="settings__button settings__button--primary"
            onClick={() => void local.add()}
            disabled={local.busy}
          >
            {local.busy ? "Reading…" : "Add a folder"}
          </button>
        </div>

        {local.addProblem && (
          <p className="settings__error">{local.addProblem}</p>
        )}

        <h3 className="label settings__alt">
          alternatively, play music from the cloud
        </h3>

        <Field
          label="Server address"
          hint="Your storage provider's WebDAV address. It starts with https://."
          note={<Suggestions
            typed={url}
            onPick={(p) => {
              if (!p.address) return;
              setUrl(p.address);
              if (p.folder) setFolder(p.folder);
            }}
          />}
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
          label="App password"
          hint="Generated by your provider for this app, not your account password."
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
                {password
                  ? "Not saved yet — press Save"
                  : "No password saved yet"}
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
          note={<FolderStart url={url} folder={folder} onPick={setFolder} />}
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

      <FoldersCard
        folders={local.folders}
        busy={local.busy}
        problem={local.forgetProblem}
        forget={(id) => void local.forget(id)}
      />

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
          // The sentence, which never changes.
          subtitle="Let Vibe DJ find tempo, key and cue points, and more"
          // The count, which changes every few seconds. Welded onto the end of
          // the sentence above it ran past the width of the row and ellipsised,
          // so the number — the only part worth watching — was the part that
          // got cut.
          detail={
            status?.running
              ? `${(progress?.done ?? status.analysed).toLocaleString()} of ${(
                  progress?.total ?? status.total
                ).toLocaleString()} — ${status.current || "working…"}`
              : status
                ? `${status.analysed.toLocaleString()} of ${status.total.toLocaleString()} done`
                : ""
          }
          footer={
            failures.length > 0 && (
              <button
                type="button"
                className="setrow__problem"
                onClick={() => setShowFailures(true)}
              >
                {failures.length === 1
                  ? "1 track could not be analysed"
                  : `${failures.length.toLocaleString()} tracks could not be analysed`}
              </button>
            )
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
          <ErrorNotice error={error.text} onDismiss={() => setError(null)} />
        )}

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

        {/*
          Sharing with your own other devices is a library setting, so it is a
          library row — Network was a section with one switch in it, and it was
          the only section on the screen whose title sat inside itself.

          `SyncPanel` now renders that row and nothing around it, so the
          wrapper here only spaces out what the panel draws below the row. It
          no longer has a card to unwrap or a heading to suppress.
        */}
        <div className="settings__shared">
          <SyncPanel />
        </div>
      </SettingGroup>

      <section className="settings__card glass">
        <h2 className="settings__section">About</h2>
        {/*
          The mark, running (POL-4).

          It used to be pinned to `LOGO_POSE`, the frame the app icon is
          exported from — which made About a picture of the icon rather than
          the thing itself. The mark is the one drawing in this app that moves,
          and About is where somebody looks at it on purpose.
        */}
        <div className="settings__lockup">
          <VaporMark size={96} state="idle" />
          <div className="settings__lockup-text">
            <span className="settings__lockup-name">Vapor Music</span>
            <span className="label">Music, continuous</span>
            {/*
              Which build this is. There is no telemetry and no crash
              reporting, on purpose, so the only channel for a fault is a
              person describing it — and "it crashed" cannot be acted on
              while "it crashed, 2.0.0, a3f9c21" names the tree. Selectable
              so it can be copied into a message.
            */}
            <span className="label settings__build" data-testid="build-stamp">
              {__APP_VERSION__} · {__APP_COMMIT__}
            </span>
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
        <button
          className="settings__licences"
          onClick={() => setLicences(true)}
        >
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

      {/*
        The supporter wall, last on the screen.
        
        Below Your Data rather than beside About, because it is the one thing
        here that asks for something rather than telling you something, and the
        end of the screen is where a person has finished reading. It renders
        nothing at all until there is a Ko-fi handle to point at.
      */}
      <SupporterPins />

      {/* The full notices, rendered from the file itself. */}
      {licences && (
        <HelpModal
          title="Licences and attributions"
          markdown={notices}
          onClose={() => setLicences(false)}
        />
      )}

      {showFailures && (
        <AnalysisFailures onClose={() => setShowFailures(false)} />
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

/**
 * Real addresses for a provider name typed into the address box (POL-3a).
 *
 * Offered rather than filled in: somebody may be typing a name that only looks
 * like a provider's, and a field that rewrites itself under the cursor is
 * worse than one that does nothing.
 */
function Suggestions({
  typed,
  onPick,
}: {
  typed: string;
  onPick: (p: Provider) => void;
}) {
  const matches = providersFor(typed);
  if (matches.length === 0) return null;

  return (
    <div className="settings__suggest">
      {matches.map((p) =>
        p.address ? (
          <button
            key={p.name}
            type="button"
            className="settings__suggest-pick"
            onClick={() => onPick(p)}
          >
            <span className="settings__suggest-name">{p.name}</span>{" "}
            <span className="settings__suggest-url">{p.address}</span>
          </button>
        ) : (
          // The whole point of naming these: somebody typing "proton" is about
          // to spend an afternoon looking for an address never issued.
          <p
            key={p.name}
            className="settings__field-note settings__field-note--warn"
          >
            {p.name} has no WebDAV
          </p>
        ),
      )}
    </div>
  );
}

/**
 * Where a known provider's files start, so only the last part is typed
 * (POL-3c). Silent once the box already starts there.
 */
function FolderStart({
  url,
  folder,
  onPick,
}: {
  url: string;
  folder: string;
  onPick: (value: string) => void;
}) {
  const provider = providerAt(url);
  const start = provider?.folder;
  if (!provider || !start || folder.startsWith(start)) return null;

  return (
    <button
      type="button"
      className="settings__suggest-pick"
      onClick={() => onPick(start)}
    >
      <span className="settings__suggest-name">{provider.name} starts at</span>{" "}
      <span className="settings__suggest-url">{start}</span>
    </button>
  );
}
