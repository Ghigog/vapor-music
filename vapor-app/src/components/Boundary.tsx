/**
 * Catches a render error so one screen cannot take the app with it.
 *
 * Without this, a single unguarded access anywhere in a screen — a `.toFixed`
 * on a value that arrived as `null` — throws during render, React unmounts the
 * whole tree, and the window goes blank. No sidebar, no message, nothing in the
 * UI to say what happened. The app looks like it failed to start.
 *
 * That failure shape cost two rounds of guessing at a bug that could have named
 * itself, which is the real argument for this component: not that screens throw,
 * but that when one does the app must still be able to say so. A blank window
 * is the least debuggable outcome available, and it is what React does by
 * default.
 *
 * Deliberately a class. Error boundaries are the one thing hooks cannot do —
 * `componentDidCatch` has no hook equivalent — so this is not a style choice.
 */
import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  /** Where this boundary sits, named for the message: "the Vibe screen". */
  where: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
  /** The component stack, which is what says *which* screen threw. */
  stack: string;
}

export class Boundary extends Component<Props, State> {
  state: State = { error: null, stack: "" };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Kept as well as shown: the message on screen is for the person, the
    // console is for whoever is looking at devtools.
    this.setState({ stack: info.componentStack ?? "" });
    console.error(`Vapor: ${this.props.where} failed to render`, error, info);
  }

  render() {
    const { error, stack } = this.state;
    if (!error) {
      return this.props.children;
    }

    return (
      <div className="boundary" role="alert">
        <h2 className="boundary__title">{this.props.where} could not be drawn</h2>
        <p className="boundary__body">
          This is a fault in Vapor, not in your library. The rest of the app is
          still working — use the sidebar to go somewhere else.
        </p>
        {/* The message verbatim. A paraphrase would lose the one detail that
            makes it possible to fix. */}
        <pre className="boundary__error">{String(error.message || error)}</pre>
        <details className="boundary__more">
          <summary>Where it happened</summary>
          <pre className="boundary__stack">{stack.trim() || "no component stack"}</pre>
        </details>
        <button
          className="boundary__retry"
          onClick={() => this.setState({ error: null, stack: "" })}
        >
          Try again
        </button>
      </div>
    );
  }
}
