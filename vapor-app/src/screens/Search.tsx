/**
 * Search.
 *
 * Distinct from the Songs table's filter box, which narrows a list in place.
 * This answers "where is that thing" — a top result, then tracks, then the
 * artists and albums the matches cluster around, so a half-remembered word
 * leads somewhere even when it was the wrong word.
 *
 * The ranking and the facets are computed in the backend against the same
 * predicate the table and smart playlists use. A search and a filter that
 * disagreed about what matches would be two answers to one question.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import * as core from "../lib/core";
import type { Row } from "../lib/core";
import { ErrorNotice, messageOf } from "../components/ErrorNotice";
import { Empty, Skeleton } from "../components/States";

/** Long enough that typing a word does not fire six searches, short enough
 *  that results feel attached to the keystroke. */
const DEBOUNCE_MS = 140;

export function Search({ onOpen }: { onOpen: (href: string) => void }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<core.SearchResults | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!query.trim()) {
      setResults(null);
      setBusy(false);
      return;
    }
    let cancelled = false;
    setBusy(true);
    setError(null);
    const timer = setTimeout(() => {
      core
        .search(query)
        .then((r) => {
          // A slower earlier search must not overwrite a faster later one.
          if (!cancelled) setResults(r);
        })
        .catch((e: unknown) => {
          // A failed search must not present as "nothing matched" — that is a
          // different answer, and a wrong one.
          if (!cancelled) {
            setResults(null);
            setError(messageOf(e));
          }
        })
        .finally(() => {
          if (!cancelled) setBusy(false);
        });
    }, DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);

  const play = useCallback(
    (href: string) => {
      const all = [
        ...(results?.top ? [results.top.href] : []),
        ...(results?.tracks.map((t) => t.href) ?? []),
      ];
      void core.playTracks(all, href);
    },
    [results],
  );

  const nothing =
    !error &&
    results !== null &&
    results.total === 0 &&
    results.playlists.length === 0 &&
    !busy;

  return (
    <div className="search">
      <div className="search__bar glass">
        <input
          ref={inputRef}
          className="search__input"
          type="search"
          value={query}
          placeholder="Search your library"
          autoComplete="off"
          spellCheck={false}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setQuery("");
          }}
        />
        {query && (
          <button
            className="search__cancel"
            onClick={() => {
              setQuery("");
              inputRef.current?.focus();
            }}
          >
            Cancel
          </button>
        )}
      </div>

      {!query.trim() && (
        <Empty
          title="Search your library"
          body="Titles, artists, albums and playlists. Everything is matched here on this device — nothing is sent anywhere."
        />
      )}

      {error && <ErrorNotice error={error} onDismiss={() => setError(null)} />}

      {query.trim() && busy && !results && !error && <Skeleton rows={5} />}

      {nothing && (
        <Empty title={`Nothing matched “${query.trim()}”`} body="Try fewer letters, or a different word." />
      )}

      {results && results.total > 0 && (
        <div className="search__results">
          {results.top && (
            <section>
              <h2 className="label search__heading">top result</h2>
              <button
                className="search__top glass"
                onClick={() => play(results.top!.href)}
                onDoubleClick={() => onOpen(results.top!.href)}
              >
                <span className="search__top-art" aria-hidden="true" />
                <span className="search__top-text">
                  <span className="search__top-title">{results.top.title}</span>
                  <span className="search__top-sub">
                    {subtitle(results.top)}
                  </span>
                  <span className="search__top-facts numeric">
                    {results.top.bpm > 0 && (
                      <span>{Math.round(results.top.bpm)} BPM</span>
                    )}
                    {results.top.key && <span>{results.top.key}</span>}
                  </span>
                </span>
              </button>
            </section>
          )}

          {results.tracks.length > 0 && (
            <section>
              <h2 className="label search__heading">
                tracks · {results.total.toLocaleString()}
              </h2>
              <ul className="search__list">
                {results.tracks.map((row) => (
                  <li key={row.href}>
                    <button
                      className="search__row"
                      onClick={() => play(row.href)}
                      onDoubleClick={() => onOpen(row.href)}
                    >
                      <span className="search__row-art" aria-hidden="true" />
                      <span className="search__row-text">
                        <span className="search__row-title">{row.title}</span>
                        <span className="search__row-sub">{subtitle(row)}</span>
                      </span>
                      <span className="search__row-bpm numeric">
                        {row.bpm > 0 ? Math.round(row.bpm) : "—"}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {(results.artists.length > 0 ||
            results.albums.length > 0 ||
            results.playlists.length > 0) && (
            <section>
              <h2 className="label search__heading">also matching</h2>
              <div className="search__chips">
                {results.artists.map((f) => (
                  <Chip
                    key={`ar-${f.label}`}
                    label={f.label}
                    kind="artist"
                    count={f.count}
                    onClick={() => setQuery(f.label)}
                  />
                ))}
                {results.albums.map((f) => (
                  <Chip
                    key={`al-${f.label}`}
                    label={f.label}
                    kind="album"
                    count={f.count}
                    onClick={() => setQuery(f.label)}
                  />
                ))}
                {results.playlists.map((p) => (
                  <Chip key={`pl-${p.id}`} label={p.name} kind="playlist" />
                ))}
              </div>
            </section>
          )}
        </div>
      )}
    </div>
  );
}

function Chip({
  label,
  kind,
  count,
  onClick,
}: {
  label: string;
  kind: "artist" | "album" | "playlist";
  count?: number;
  onClick?: () => void;
}) {
  return (
    <button className="search__chip" onClick={onClick} disabled={!onClick}>
      <span className="search__chip-kind label">{kind}</span>
      <span>{label}</span>
      {count !== undefined && (
        <span className="search__chip-count numeric">{count}</span>
      )}
    </button>
  );
}

/** Artist and album, minus whatever is unknown — the table's dash convention
 *  would read as noise repeated across a list of results. */
function subtitle(row: Row): string {
  const parts = [
    row.artistSource === "unknown" ? "" : row.artist,
    row.albumSource === "unknown" ? "" : row.album,
  ].filter(Boolean);
  return parts.length ? parts.join(" · ") : "—";
}
