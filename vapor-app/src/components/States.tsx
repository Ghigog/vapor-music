/**
 * Loading and Empty.
 *
 * The design lists these as screens; they are really states that every other
 * screen falls into, so they live here as components rather than as two more
 * routes. Building them twelve times over is how a set of screens stops
 * agreeing with itself about what "nothing here" looks like.
 *
 * The mark is the spinner — the design says so explicitly, and it is the one
 * piece of motion the product already owns.
 */

import { VaporMark } from "./VaporMark";

export function Loading({
  label = "Listening…",
  detail,
}: {
  label?: string;
  /** The honest second line: what is being counted, and where. */
  detail?: string;
}) {
  return (
    <div className="state">
      {/* `thinking` rather than `idle`: the mark tightens while it works, and
          a spinner that does not spin is not a spinner. */}
      <VaporMark size={96} state="thinking" />
      <p className="state__title">{label}</p>
      {detail && <p className="state__detail numeric">{detail}</p>}
    </div>
  );
}

export function Empty({
  title,
  body,
  action,
}: {
  title: string;
  body?: string;
  action?: { label: string; onClick: () => void };
}) {
  return (
    <div className="state">
      <VaporMark size={96} state="idle" />
      <p className="state__title">{title}</p>
      {body && <p className="state__body">{body}</p>}
      {action && (
        <button className="state__button" onClick={action.onClick}>
          {action.label}
        </button>
      )}
    </div>
  );
}

/**
 * A row of grey bars standing in for content that is on its way.
 *
 * Used where a *shape* is already known — a table of rows, a list of results —
 * because a skeleton that matches the thing it precedes reads as the content
 * arriving, and one that does not reads as a second kind of loading.
 */
export function Skeleton({ rows = 6 }: { rows?: number }) {
  return (
    <div className="skeleton" aria-hidden="true">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="skeleton__row">
          <div className="skeleton__art" />
          <div className="skeleton__lines">
            {/* Varied widths: a stack of identical bars reads as a pattern
                rather than as text. */}
            <div className="skeleton__line" style={{ width: `${58 + ((i * 13) % 30)}%` }} />
            <div className="skeleton__line skeleton__line--short" style={{ width: `${28 + ((i * 7) % 18)}%` }} />
          </div>
        </div>
      ))}
    </div>
  );
}
