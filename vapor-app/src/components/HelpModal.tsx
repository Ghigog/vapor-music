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
import { createPortal } from "react-dom";

import { useDialog } from "./dialog";
import type { ReactNode } from "react";
import { overlayRoot } from "./overlay";

/**
 * Inline markdown: `**bold**` and `` `code` ``.
 *
 * Split on both markers at once rather than running two passes, so a bold
 * span containing code (or the reverse) cannot have one marker consumed and
 * the other left as literal asterisks on screen.
 */
function inline(text: string, keyBase: string): ReactNode[] {
  const parts: ReactNode[] = [];
  // `**bold**`, `` `code` ``, `[text](url)` and bare `<https://…>`. One pass,
  // so a marker inside another cannot be half-consumed.
  const pattern =
    /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\((https?:[^)]+)\)|<(https?:[^>]+)>/g;
  let last = 0;
  let match: RegExpExecArray | null;
  let i = 0;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > last) parts.push(text.slice(last, match.index));
    if (match[1] !== undefined) {
      parts.push(<strong key={`${keyBase}-b${i}`}>{match[1]}</strong>);
    } else if (match[2] !== undefined) {
      parts.push(
        <code className="help__code" key={`${keyBase}-c${i}`}>
          {match[2]}
        </code>,
      );
    } else {
      // `noreferrer` and a new tab: these are third-party sites, and the sheet
      // must not navigate the app away from itself.
      const href = match[4] ?? match[5] ?? "";
      parts.push(
        <a
          className="help__link"
          key={`${keyBase}-a${i}`}
          href={href}
          target="_blank"
          rel="noreferrer"
        >
          {match[3] ?? href}
        </a>,
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

  /*
   * Tables, gathered as rows and emitted whole.
   *
   * `THIRD_PARTY_NOTICES.md` states several attributions as tables — which
   * font, whose copyright, from where — and rendered a line at a time those
   * arrive as literal pipes. A licence notice that is unreadable is not a
   * notice, so the renderer had to learn the one construct the document needs.
   *
   * The `|---|---|` separator row is dropped rather than drawn; alignment
   * markers in it are ignored, because nothing here uses them.
   */
  let table: string[][] = [];

  const flushTable = () => {
    if (table.length === 0) return;
    const rows = table;
    table = [];
    const [head, ...body] = rows;
    out.push(
      <div className="help__table-wrap" key={`tw-${out.length}`}>
        <table className="help__table">
          {head && (
            <thead>
              <tr>
                {head.map((cell, i) => (
                  <th key={i}>{inline(cell, `th-${out.length}-${i}`)}</th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {body.map((row, r) => (
              <tr key={r}>
                {row.map((cell, c) => (
                  <td key={c}>{inline(cell, `td-${out.length}-${r}-${c}`)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>,
    );
  };

  /*
   * Blockquotes, including GitHub's `> [!NOTE]` callouts.
   *
   * The notices file uses them for the things a reader most needs to see — the
   * MPL explanation, the Reserved Font Name reasoning — so dropping the marker
   * and running them into the body would bury exactly the wrong paragraphs.
   */
  // Paragraphs, each a run of wrapped lines — a blockquote is hard-wrapped in
  // the source exactly as body text is, and a bare `>` is what separates one
  // paragraph from the next. Splitting per line breaks sentences in half.
  let quote: string[][] = [];
  let quoteKind = "";

  const quoteLine = (text: string) => {
    if (!text.trim()) {
      if (quote.length && quote[quote.length - 1]?.length) quote.push([]);
      return;
    }
    if (!quote.length) quote.push([]);
    quote[quote.length - 1]?.push(text.trim());
  };

  const flushQuote = () => {
    if (quote.length === 0) return;
    const body = quote.filter((para) => para.length).map((para) => para.join(" "));
    const kind = quoteKind;
    quote = [];
    quoteKind = "";
    if (body.length === 0) return;
    out.push(
      <blockquote
        className={"help__quote" + (kind ? ` help__quote--${kind}` : "")}
        key={`q-${out.length}`}
      >
        {kind && <span className="help__quote-kind">{kind}</span>}
        {body.map((line, i) => (
          <p key={i}>{inline(line, `q-${out.length}-${i}`)}</p>
        ))}
      </blockquote>,
    );
  };

  const flush = () => {
    flushParagraph();
    flushList();
    flushTable();
    flushQuote();
  };

  lines.forEach((raw) => {
    const line = raw.trim();

    if (line === "") {
      flush();
      return;
    }

    // A table row. The separator is structure, not content.
    if (line.startsWith("|") && line.endsWith("|")) {
      flushParagraph();
      flushList();
      flushQuote();
      const cells = line
        .slice(1, -1)
        .split("|")
        .map((c) => c.trim());
      if (!cells.every((c) => /^:?-{2,}:?$/.test(c))) table.push(cells);
      return;
    }

    if (line.startsWith(">")) {
      flushParagraph();
      flushList();
      flushTable();
      const body = line.replace(/^>\s?/, "");
      const callout = /^\[!(\w+)\]$/.exec(body);
      if (callout) {
        // Opens a new quote, so two callouts in a row do not merge.
        flushQuote();
        quoteKind = (callout[1] ?? "").toLowerCase();
      } else {
        quoteLine(body);
      }
      return;
    }

    if (line.startsWith("- ")) {
      flushParagraph();
      flushTable();
      flushQuote();
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
      // Ordinary prose. A list, table or quote ends where it begins.
      flushList();
      flushTable();
      flushQuote();
      paragraph.push(line);
    }
  });

  flush();
  return out;
}

/**
 * The sheet itself, without an opinion about what is in it.
 *
 * Extracted when a second thing needed a modal — the list of tracks analysis
 * could not describe. The parts worth having in one place are the ones easy to
 * leave out of a second copy: Escape closes, focus lands on the close button
 * rather than after the whole document, and the backdrop dismisses only when
 * it is itself the target, so a drag that starts on the text and ends outside
 * does not throw away what you were reading.
 */
export function Modal({
  title,
  children,
  onClose,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);

  useDialog(panel, onClose);

  useEffect(() => {
    closeRef.current?.focus();
  }, [onClose]);

  return createPortal(
    <div
      className="help__backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={panel}
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
            aria-label={`Close ${title}`}
          >
            ×
          </button>
        </header>
        <div className="help__body">{children}</div>
      </div>
    </div>,
    overlayRoot(),
  );
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
  return (
    <Modal title={title} onClose={onClose}>
      {render(markdown)}
    </Modal>
  );
}
