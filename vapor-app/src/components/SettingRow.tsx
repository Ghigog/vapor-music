/**
 * One row of Settings.
 *
 * ## Why the screen is a list now
 *
 * It was a stack of glass cards, each with a heading, two or three paragraphs
 * of reasoning, and a control somewhere inside. Every card explained itself at
 * the length the decision deserved when it was written, which added up to a
 * screen you scrolled through rather than used — and the thing you came to
 * press was never where your eye landed.
 *
 * A row states what the control does, says the one fact that decides whether to
 * touch it, and puts the control on the right where every row keeps it. The
 * reasoning did not become less true; it moved to where reasoning belongs,
 * which is beside the code it governs.
 *
 * ## The subtitle is the state, not a description
 *
 * "50 of 500 fetched" tells you whether pressing this would do anything.
 * "Fetches lyrics and artwork" does not — the title already said that. Where a
 * row has nothing useful to report it should say what pressing it costs.
 */

import type { ReactNode } from "react";

export function SettingRow({
  title,
  subtitle,
  children,
  onClick,
}: {
  title: string;
  /** The state that decides whether to touch this. Not a description. */
  subtitle?: ReactNode;
  /** The control: a switch, a button, whatever the row acts through. */
  children?: ReactNode;
  /**
   * Makes the whole row the control.
   *
   * For rows that *are* a single action — Analyse, Fetch — the row is the
   * button, because a title and a separate small button beside it are two
   * targets for one intention.
   */
  onClick?: (() => void) | undefined;
}) {
  const body = (
    <>
      <span className="setrow__text">
        <span className="setrow__title">{title}</span>
        {subtitle !== undefined && subtitle !== "" && (
          <span className="setrow__sub">{subtitle}</span>
        )}
      </span>
      {children && <span className="setrow__control">{children}</span>}
    </>
  );

  if (onClick) {
    return (
      <button type="button" className="setrow setrow--pressable" onClick={onClick}>
        {body}
      </button>
    );
  }
  return <div className="setrow">{body}</div>;
}

/** A group of rows under one heading. */
export function SettingGroup({
  title,
  children,
}: {
  title?: string;
  children: ReactNode;
}) {
  return (
    <section className="setgroup">
      {title && <h2 className="setgroup__title label">{title}</h2>}
      <div className="setgroup__rows">{children}</div>
    </section>
  );
}
