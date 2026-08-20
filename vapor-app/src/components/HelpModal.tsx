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
 *
 * ## Runs of lines are one block, which is what markdown means
 *
 * This used to emit a `<p>` per source line. The document is hard-wrapped at
 * about eighty columns, so every sentence that crossed a wrap was rendered as
 * two paragraphs with a gap between them — "…and were" / "measured against a
 * real 534-track library" — and every bullet with a continuation line was
 * broken into a bullet plus a stray paragraph beneath the list, which is where
 * the lone "in." on the Vibe help sheet came from.
 *
 * Consecutive non-blank lines are therefore joined, and a line indented under
 * a bullet continues that bullet. A blank line is what ends a block, exactly
 * as markdown says.
 */
function render(markdown: string): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = markdown.split("\n");
  let bullets: string[] = [];
  let paragraph: string[] = [];

  const flushParagraph = () => {
    if (paragraph.length === 0) return;
    const text = paragraph.join(" ");
    paragraph = [];
    out.push(<p key={`p-${out.length}`}>{inline(text, `p-${out.length}`)}</p>);
  };

  const flushList = () => {
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

  const flush = () => {
    flushParagraph();
    flushList();
  };

  lines.forEach((raw) => {
    const line = raw.trim();

    if (line === "") {
      flush();
      return;
    }

    if (line.startsWith("- ")) {
      flushParagraph();
      bullets.push(line.slice(2));
      return;
    }

    // A wrapped bullet: indented in the source, and belonging to the item
    // above rather than starting anything of its own.
    if (bullets.length > 0 && /^\s/.test(raw)) {
      bullets[bullets.length - 1] += ` ${line}`;
      return;
    }

    if (line.startsWith("### ")) {
      flush();
      out.push(<h4 key={`h-${out.length}`}>{inline(line.slice(4), `h-${out.length}`)}</h4>);
    } else if (line.startsWith("## ")) {
      flush();
      out.push(<h3 key={`h-${out.length}`}>{inline(line.slice(3), `h-${out.length}`)}</h3>);
    } else if (line.startsWith("# ")) {
      flush();
      out.push(<h2 key={`h-${out.length}`}>{inline(line.slice(2), `h-${out.length}`)}</h2>);
    } else if (/^-{3,}$/.test(line)) {
      flush();
      out.push(<hr className="help__rule" key={`hr-${out.length}`} />);
    } else {
      // Ordinary prose. A list ends where unindented prose begins.
      flushList();
      paragraph.push(line);
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
