/**
 * The help sheet, ported from the Godot original.
 *
 * `vibe_screen.gd` had a help button that opened a modal and rendered
 * `res://docs/ai_dj_workflow.md` — the spec itself, converted to BBCode at
 * runtime. That is the part worth keeping: the help cannot drift from the
 * behaviour it describes, because it *is* the document the behaviour was
 * written from. A hand-written help panel would be a third copy of the rules,
 * and the third copy is the one that goes stale.
 *
 * So the markdown is imported verbatim (`?raw`) rather than retyped here.
 * Changing the spec changes the help, in the same commit, with no step in
 * between that anyone can forget.
 */

import { useEffect, useRef } from "react";
import type { ReactNode } from "react";

/**
 * Inline markdown: `**bold**` and `` `code` ``.
 *
 * Split on both markers at once rather than running two passes, so a bold
 * span containing code (or the reverse) cannot have one marker consumed and
 * the other left as literal asterisks on screen.
 */
function inline(text: string, keyBase: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const pattern = /\*\*([^*]+)\*\*|`([^`]+)`/g;
  let last = 0;
  let match: RegExpExecArray | null;
  let i = 0;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > last) parts.push(text.slice(last, match.index));
    if (match[1] !== undefined) {
      parts.push(<strong key={`${keyBase}-b${i}`}>{match[1]}</strong>);
    } else {
      parts.push(
        <code className="help__code" key={`${keyBase}-c${i}`}>
          {match[2]}
        </code>,
      );
    }
    last = match.index + match[0].length;
    i++;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

/**
 * Block markdown: headings, bullet lists, horizontal rules, paragraphs.
 *
 * Only the constructs `ai_dj_workflow.md` actually uses. An unknown construct
 * renders as a paragraph rather than disappearing — help text that silently
 * drops a line is worse than help text with a stray hyphen in it.
 */
function render(markdown: string): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = markdown.split("\n");
  let bullets: string[] = [];

  const flush = () => {
    if (bullets.length === 0) return;
    const items = bullets;
    bullets = [];
    out.push(
      <ul className="help__list" key={`ul-${out.length}`}>
        {items.map((item, i) => (
          <li key={i}>{inline(item, `li-${out.length}-${i}`)}</li>
        ))}
      </ul>,
    );
  };

  lines.forEach((raw, index) => {
    const line = raw.trim();

    if (line === "") {
      flush();
      return;
    }

    if (line.startsWith("- ")) {
      bullets.push(line.slice(2));
      return;
    }

    flush();

    if (line.startsWith("### ")) {
      out.push(<h4 key={index}>{inline(line.slice(4), `h${index}`)}</h4>);
    } else if (line.startsWith("## ")) {
      out.push(<h3 key={index}>{inline(line.slice(3), `h${index}`)}</h3>);
    } else if (line.startsWith("# ")) {
      out.push(<h2 key={index}>{inline(line.slice(2), `h${index}`)}</h2>);
    } else if (/^-{3,}$/.test(line)) {
      out.push(<hr className="help__rule" key={index} />);
    } else {
      out.push(<p key={index}>{inline(line, `p${index}`)}</p>);
    }
  });

  flush();
  return out;
}

export function HelpModal({
  title,
  markdown,
  onClose,
}: {
  title: string;
  markdown: string;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    // Focus the close button, so the sheet is dismissable from the keyboard
    // the moment it opens rather than after tabbing through all of it.
    closeRef.current?.focus();

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="help__backdrop"
      // The backdrop closes it, but only when the backdrop itself is the
      // target: a drag that starts on the text and ends outside must not
      // dismiss the thing being read.
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="help__panel glass"
        role="dialog"
        aria-modal="true"
        aria-label={title}
      >
        <header className="help__head">
          <h2 className="help__title">{title}</h2>
          <button
            ref={closeRef}
            className="help__close"
            onClick={onClose}
            aria-label="Close help"
          >
            ×
          </button>
        </header>
        <div className="help__body">{render(markdown)}</div>
      </div>
    </div>
  );
}
